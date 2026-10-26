package com.p2pchat.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SettingsCardsTest {

    @get:Rule
    val compose = createComposeRule()

    @Test
    fun versionCardShowsInstalledVersion() {
        compose.setContent {
            MaterialTheme {
                SettingsVersionCard(version = "1.2.3", update = null, updateCheckFailed = false, onClick = {})
            }
        }

        compose.onNodeWithText("v1.2.3").assertIsDisplayed()
    }

    @Test
    fun toggleCardReportsNewValue() {
        var changed = false
        compose.setContent {
            MaterialTheme {
                SettingsToggleCard(
                    title = "Keep alive",
                    description = "Background connections",
                    checked = false,
                    onChange = { changed = it },
                )
            }
        }

        compose.onNode(isToggleable()).performClick()
        compose.waitForIdle()
        assertTrue(changed)
    }

    @Test
    fun checkedToggleRendersOn() {
        compose.setContent {
            MaterialTheme {
                SettingsToggleCard(
                    title = "Debug logs",
                    description = "Verbose logs",
                    checked = true,
                    onChange = {},
                )
            }
        }

        compose.onNode(isToggleable()).assertIsOn()
    }

    @Test
    fun linkCardIsClickable() {
        var clicked = false
        compose.setContent {
            MaterialTheme {
                SettingsLinkCard("Privacy policy") { clicked = true }
            }
        }

        compose.onNodeWithText("Privacy policy").performClick()
        compose.waitForIdle()
        assertTrue(clicked)
    }
}
