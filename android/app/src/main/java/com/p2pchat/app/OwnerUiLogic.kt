package com.p2pchat.app

/** Иконки управления владельцем показываем всем, кроме владельца. */
internal fun ownerActionsVisible(isOwner: Boolean): Boolean = !isOwner

/** Действие доступно, только если оно разрешено и нет активных выборов. */
internal fun ownerActionEnabled(allowed: Boolean, electionActive: Boolean): Boolean =
    allowed && !electionActive

/** Подсказка-иконка в свёрнутой шапке. */
internal fun ownerManagementHintVisible(isOwner: Boolean, expanded: Boolean): Boolean =
    ownerActionsVisible(isOwner) && !expanded

/** Окно голосования: в паре challenge не ждёт окончания окна заявок. */
internal fun electionVotingOpen(
    trigger: Int,
    memberCount: Int,
    nowSec: ULong,
    voteStart: ULong,
    endsAt: ULong,
): Boolean {
    if (nowSec >= endsAt) return false
    if (nowSec >= voteStart) return true
    return trigger == 1 && memberCount <= 2
}
