package com.ombhrum.fabushi.e2e.harness

import android.app.Application
import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageManager
import java.io.File
import java.nio.file.Files
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Pure-JVM in-memory [SharedPreferences] used by [TestFabushiApplication].
 * Allows opaque-box inspection of persisted keys (e.g. verifying zero plaintext PIN storage).
 */
class InMemorySharedPreferences : SharedPreferences {
    private val data = ConcurrentHashMap<String, Any>()
    private val listeners = CopyOnWriteArrayList<SharedPreferences.OnSharedPreferenceChangeListener>()

    override fun getAll(): MutableMap<String, *> = HashMap(data)
    override fun getString(key: String, defValue: String?): String? = data[key] as? String ?: defValue

    @Suppress("UNCHECKED_CAST")
    override fun getStringSet(key: String, defValues: MutableSet<String>?): MutableSet<String>? =
        (data[key] as? Set<String>)?.toMutableSet() ?: defValues

    override fun getInt(key: String, defValue: Int): Int = data[key] as? Int ?: defValue
    override fun getLong(key: String, defValue: Long): Long = data[key] as? Long ?: defValue
    override fun getFloat(key: String, defValue: Float): Float = data[key] as? Float ?: defValue
    override fun getBoolean(key: String, defValue: Boolean): Boolean = data[key] as? Boolean ?: defValue
    override fun contains(key: String): Boolean = data.containsKey(key)

    override fun edit(): SharedPreferences.Editor = object : SharedPreferences.Editor {
        private val pending = LinkedHashMap<String, Any?>()
        private var clearAll = false

        override fun putString(key: String, value: String?): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putStringSet(key: String, values: MutableSet<String>?): SharedPreferences.Editor =
            apply { pending[key] = values?.toSet() }
        override fun putInt(key: String, value: Int): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putLong(key: String, value: Long): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putFloat(key: String, value: Float): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putBoolean(key: String, value: Boolean): SharedPreferences.Editor = apply { pending[key] = value }
        override fun remove(key: String): SharedPreferences.Editor = apply { pending[key] = null }
        override fun clear(): SharedPreferences.Editor = apply { clearAll = true }

        override fun commit(): Boolean {
            apply()
            return true
        }

        override fun apply() {
            if (clearAll) data.clear()
            pending.forEach { (k, v) ->
                if (v == null) data.remove(k) else data[k] = v
                listeners.forEach { it.onSharedPreferenceChanged(this@InMemorySharedPreferences, k) }
            }
        }
    }

    override fun registerOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener) {
        listeners.add(listener)
    }

    override fun unregisterOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener) {
        listeners.remove(listener)
    }
}

/**
 * Pure-JVM [Application] subclass isolating per-test SQLite/files/cache directories,
 * in-memory [SharedPreferences], and configurable Android runtime permissions (default empty -> DENIED).
 */
class TestFabushiApplication(
    val rootDir: File = Files.createTempDirectory("fabushi-e2e-").toFile(),
    val grantedPermissions: MutableSet<String> = mutableSetOf(),
    private val sharedPrefsStore: ConcurrentHashMap<String, InMemorySharedPreferences> = ConcurrentHashMap(),
) : Application() {
    private val filesDirectory = File(rootDir, "files").apply { mkdirs() }
    private val cacheDirectory = File(rootDir, "cache").apply { mkdirs() }
    private val externalDirectory = File(rootDir, "external").apply { mkdirs() }

    override fun getApplicationContext(): Context = this
    override fun getFilesDir(): File = filesDirectory
    override fun getCacheDir(): File = cacheDirectory
    override fun getExternalFilesDir(type: String?): File = externalDirectory

    override fun getSharedPreferences(name: String, mode: Int): SharedPreferences =
        sharedPrefsStore.computeIfAbsent(name) { InMemorySharedPreferences() }

    override fun checkPermission(permission: String, pid: Int, uid: Int): Int =
        if (permission in grantedPermissions) PackageManager.PERMISSION_GRANTED else PackageManager.PERMISSION_DENIED

    fun telegramPrefs(): InMemorySharedPreferences =
        sharedPrefsStore.computeIfAbsent("fabushi.telegram.prefs") { InMemorySharedPreferences() }

    /**
     * Creates a fresh [TestFabushiApplication] pointing at the SAME [rootDir] (SQLite WAL files)
     * with a fresh [SharedPreferences] store (simulating cold restart, optionally inheriting prefs).
     */
    fun coldRestartApp(preservePrefs: Boolean = false): TestFabushiApplication =
        TestFabushiApplication(
            rootDir = rootDir,
            grantedPermissions = grantedPermissions.toMutableSet(),
            sharedPrefsStore = if (preservePrefs) sharedPrefsStore else ConcurrentHashMap(),
        )

    fun cleanup() {
        rootDir.deleteRecursively()
    }
}
