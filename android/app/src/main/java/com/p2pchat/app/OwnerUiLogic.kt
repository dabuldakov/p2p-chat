package com.p2pchat.app

/** Иконки управления владельцем показываем всем, кроме владельца. */
internal fun ownerActionsVisible(isOwner: Boolean): Boolean = !isOwner

/** Действие доступно, только если оно разрешено и нет активных выборов. */
internal fun ownerActionEnabled(allowed: Boolean, electionActive: Boolean): Boolean =
    allowed && !electionActive

/** Подсказка-иконка в свёрнутой шапке. */
internal fun ownerManagementHintVisible(isOwner: Boolean, expanded: Boolean): Boolean =
    ownerActionsVisible(isOwner) && !expanded
