package com.wangzan101.mika.task

import java.lang.reflect.Modifier
import com.wangzan101.mika.MikaApp
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ExternalScheduleActionsTest {
    @Test
    fun acceptsBothMikaAndFabushiScheduleActions() {
        assertTrue("com.wangzan101.mika.SCHEDULE" in ExternalSchedule.ACTIONS)
        assertTrue("com.ombhrum.fabushi.SCHEDULE" in ExternalSchedule.ACTIONS)
        assertFalse("android.intent.action.VIEW" in ExternalSchedule.ACTIONS)
    }

    @Test
    fun mikaAppIsOpenForSubclassingByHostApplication() {
        assertFalse("MikaApp must be open so FabushiApplication can extend it", Modifier.isFinal(MikaApp::class.java.modifiers))
    }
}
