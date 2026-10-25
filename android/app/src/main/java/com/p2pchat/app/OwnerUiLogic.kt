package com.p2pchat.app

/** Показывать ли иконки запроса/вызова владельца в списке участников. */
internal fun ownerActionsVisible(isOwner: Boolean, electionActive: Boolean): Boolean =
    !isOwner && !electionActive

/** Показывать ли подсказку-иконку в свёрнутой шапке, чтобы открыть управление. */
internal fun ownerManagementHintVisible(
    isOwner: Boolean,
    electionActive: Boolean,
    expanded: Boolean,
): Boolean = ownerActionsVisible(isOwner, electionActive) && !expanded
