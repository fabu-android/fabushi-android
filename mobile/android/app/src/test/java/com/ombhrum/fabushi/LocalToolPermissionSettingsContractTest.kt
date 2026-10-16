package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidLocalToolPermission
import com.ombhrum.fabushi.androidpreload.runtime.AndroidLocalToolPermissionSettings
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LocalToolPermissionSettingsContractTest {
    @Test
    fun wireValuesRoundTripWithoutCollapsingPolicyLevels() {
        assertEquals(AndroidLocalToolPermission.NEVER, AndroidLocalToolPermission.fromWire("never"))
        assertEquals(AndroidLocalToolPermission.ASK, AndroidLocalToolPermission.fromWire("ask"))
        assertEquals(AndroidLocalToolPermission.ALWAYS, AndroidLocalToolPermission.fromWire("always"))
    }

    @Test
    fun settingsKeepPreferenceSeparateFromEffectiveManagedValue() {
        val settings = AndroidLocalToolPermissionSettings(
            permission = AndroidLocalToolPermission.ASK,
            preference = AndroidLocalToolPermission.ALWAYS,
            ceiling = AndroidLocalToolPermission.ASK,
        )
        assertEquals(AndroidLocalToolPermission.ASK, settings.permission)
        assertEquals(AndroidLocalToolPermission.ALWAYS, settings.preference)
        assertEquals(AndroidLocalToolPermission.ASK, settings.ceiling)
        assertEquals(0, settings.settledPendingRemoteApprovals)

        val unmanaged = settings.copy(ceiling = null)
        assertNull(unmanaged.ceiling)
    }
}
