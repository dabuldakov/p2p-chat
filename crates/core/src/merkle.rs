//! Merkle-дерево над множеством id сообщений группы.
//!
//! Зачем: перед тяжёлым обменом сообщениями два узла сравнивают **корни**
//! (`digest`). Если корни совпали — реплики идентичны, синк не нужен (O(1)).
//! Если нет — обмениваются списками id и добирают недостающее.
//!
//! Реализация — с балансировкой дублированием последнего узла (как в Bitcoin);
//! листья сортируются по id, поэтому дерево не зависит от порядка вставки.

use sha2::{Digest, Sha256};

use crate::MessageId;

pub type Hash = [u8; 32];

#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// Отсортированные id (листья).
    ids: Vec<MessageId>,
    /// levels[0] — хеши листьев, levels[last] — корень.
    levels: Vec<Vec<Hash>>,
}

impl MerkleTree {
    /// Построить дерево из id (порядок не важен).
    pub fn build(mut ids: Vec<MessageId>) -> Self {
        ids.sort();
        ids.dedup();

        let mut leaves: Vec<Hash> = ids.iter().map(|id| hash_leaf(id)).collect();
        if leaves.is_empty() {
            return Self { ids, levels: vec![vec![]] };
        }

        let mut levels = vec![leaves.clone()];
        while leaves.len() > 1 {
            let mut next = Vec::with_capacity(leaves.len().div_ceil(2));
            let mut i = 0;
            while i < leaves.len() {
                let left = leaves[i];
                let right = if i + 1 < leaves.len() { leaves[i + 1] } else { leaves[i] };
                next.push(hash_node(&left, &right));
                i += 2;
            }
            levels.push(next.clone());
            leaves = next;
        }

        Self { ids, levels }
    }

    /// Корень. Пустое дерево → нулевой хеш.
    pub fn root(&self) -> Hash {
        self.levels
            .last()
            .and_then(|l| l.first().copied())
            .unwrap_or([0u8; 32])
    }

    pub fn ids(&self) -> &[MessageId] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Совпадают ли множества (по корню).
    pub fn matches(&self, other: &MerkleTree) -> bool {
        self.root() == other.root() && self.len() == other.len()
    }

    /// Какие id есть у нас, но отсутствуют у `other`.
    /// Используется, чтобы отдать собеседнику ровно то, чего у него нет.
    pub fn missing_from(&self, other: &MerkleTree) -> Vec<MessageId> {
        let mut out = Vec::new();
        let (mut i, mut j) = (0usize, 0usize);
        while i < self.ids.len() {
            if j >= other.ids.len() {
                out.push(self.ids[i].clone());
                i += 1;
            } else {
                match self.ids[i].cmp(&other.ids[j]) {
                    std::cmp::Ordering::Equal => {
                        i += 1;
                        j += 1;
                    }
                    std::cmp::Ordering::Less => {
                        out.push(self.ids[i].clone());
                        i += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        j += 1;
                    }
                }
            }
        }
        out
    }
}

fn hash_leaf(id: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update([0x00]);
    h.update(id);
    h.finalize().into()
}

fn hash_node(left: &Hash, right: &Hash) -> Hash {
    let mut h = Sha256::new();
    h.update([0x01]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&[u8]]) -> Vec<MessageId> {
        v.iter().map(|x| x.to_vec()).collect()
    }

    #[test]
    fn empty_root_is_zero() {
        let t = MerkleTree::build(vec![]);
        assert_eq!(t.root(), [0u8; 32]);
        assert!(t.is_empty());
    }

    #[test]
    fn order_independent() {
        let a = MerkleTree::build(ids(&[b"a", b"b", b"c"]));
        let b = MerkleTree::build(ids(&[b"c", b"a", b"b"]));
        assert_eq!(a.root(), b.root());
    }

    #[test]
    fn different_sets_differ() {
        let a = MerkleTree::build(ids(&[b"a", b"b"]));
        let b = MerkleTree::build(ids(&[b"a", b"c"]));
        assert_ne!(a.root(), b.root());
    }

    #[test]
    fn missing_from_computes_diff() {
        let a = MerkleTree::build(ids(&[b"a", b"b", b"c"]));
        let b = MerkleTree::build(ids(&[b"b", b"d"]));
        let missing = a.missing_from(&b);
        assert_eq!(missing, ids(&[b"a", b"c"]));
    }
}