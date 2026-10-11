package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.ComputerRebuildBoxIngress
import com.ombhrum.fabushi.androidmain.coordinator.ComputerRebuildStateOwner
import com.ombhrum.fabushi.androidmain.coordinator.ComputerRebuildStateStore
import com.ombhrum.fabushi.androidmain.coordinator.ComputerRebuildSnapshot
import com.ombhrum.fabushi.androidmain.coordinator.projectForeverBoxRebuildEvent
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ComputerUpdateProjectionTest {
    private class MemoryStore : ComputerRebuildStateStore {
        var value: ComputerRebuildSnapshot? = null
        override fun read(): ComputerRebuildSnapshot? = value
        override fun write(snapshot: ComputerRebuildSnapshot?) { value = snapshot }
    }

    @Test
    fun foreverBoxAvailabilityIsProjectedAndDurablyOwned() {
        val ingress = projectForeverBoxRebuildEvent(
            JSONObject(
                """{"agentId":"forever-box","state":"running","vncUrl":"wss://viewer","imageUpdateAvailable":true}""",
            ),
        )
        assertEquals(
            ComputerRebuildBoxIngress(
                boxId = "forever-box",
                phase = "running",
                pullPercent = null,
                imageUpdateAvailable = true,
            ),
            ingress,
        )

        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store, processGeneration = 5)
        owner.observeAccount(8)
        owner.observeBox(
            accountEpoch = 8,
            boxId = ingress!!.boxId,
            phase = ingress.phase,
            pullPercent = ingress.pullPercent,
            imageUpdateAvailable = ingress.imageUpdateAvailable,
        )
        assertEquals(true, owner.snapshot(8).imageUpdateAvailable)
        assertEquals(true, store.value?.imageUpdateAvailable)

        // A later status frame that omits the availability bit must not erase the last canonical
        // observation. Desktop's box store treats undefined availability as "no new fact".
        owner.observeBox(8, "forever-box", "running")
        assertEquals(true, owner.snapshot(8).imageUpdateAvailable)
    }

    @Test
    fun paletteComputerUpdateFailsClosedUnlessIdleSingleAgentAndAvailable() {
        val ready = CommandPaletteComputerUpdateSnapshot(
            imageUpdateAvailable = true,
            pending = false,
            outcomeUnknown = false,
            rebuildKind = null,
        )
        assertEquals(
            CommandPaletteComputerUpdateAction.READY,
            commandPaletteComputerUpdateAction(false, 0, ready),
        )
        assertNull(commandPaletteComputerUpdateAction(null, 0, ready))
        assertNull(commandPaletteComputerUpdateAction(true, 0, ready))
        assertNull(commandPaletteComputerUpdateAction(false, 1, ready))
        assertNull(commandPaletteComputerUpdateAction(false, 0, ready.copy(imageUpdateAvailable = false)))
        assertNull(commandPaletteComputerUpdateAction(false, 0, ready.copy(imageUpdateAvailable = null)))
        assertNull(commandPaletteComputerUpdateAction(false, 0, ready.copy(pending = true)))
        assertNull(commandPaletteComputerUpdateAction(false, 0, ready.copy(outcomeUnknown = true)))
        assertNull(commandPaletteComputerUpdateAction(false, 0, ready.copy(rebuildKind = "update")))
    }

    @Test
    fun typedMobileProjectionPreservesUnknownAndBlocksAmbiguousEpisodes() {
        val unknown = projectMobileComputerRebuildStatus(
            JSONObject("""{"pending":false,"outcomeUnknown":true,"kind":null}"""),
        )
        assertNull(unknown.imageUpdateAvailable)
        assertTrue(unknown.outcomeUnknown)
        assertNull(
            commandPaletteComputerUpdateAction(
                activeAgentIsGroup = false,
                workingAgentCount = 0,
                snapshot = CommandPaletteComputerUpdateSnapshot(
                    imageUpdateAvailable = unknown.imageUpdateAvailable,
                    pending = unknown.pending,
                    outcomeUnknown = unknown.outcomeUnknown,
                    rebuildKind = unknown.kind,
                ),
            ),
        )

        val ready = projectMobileComputerRebuildStatus(
            JSONObject(
                """{"imageUpdateAvailable":true,"pending":false,"outcomeUnknown":false,"kind":null}""",
            ),
        )
        assertEquals(true, ready.imageUpdateAvailable)
        assertEquals(
            CommandPaletteComputerUpdateAction.READY,
            commandPaletteComputerUpdateAction(
                activeAgentIsGroup = false,
                workingAgentCount = 0,
                snapshot = CommandPaletteComputerUpdateSnapshot(
                    imageUpdateAvailable = ready.imageUpdateAvailable,
                    pending = ready.pending,
                    outcomeUnknown = ready.outcomeUnknown,
                    rebuildKind = ready.kind,
                ),
            ),
        )
    }

    @Test
    fun malformedAvailabilityDoesNotManufactureAnUpdate() {
        val ingress = projectForeverBoxRebuildEvent(
            JSONObject(
                """{"agentId":"forever-box","state":"running","imageUpdateAvailable":"yes"}""",
            ),
        )
        assertTrue(ingress != null)
        assertNull(ingress?.imageUpdateAvailable)
    }
}
