package com.wangzan101.mika

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/** 清单里会让进程一启动就崩或污染宿主 APK 合并的配置,在电脑上先拦住。 */
class ManifestProvidersTest {

    private val androidNs = "http://schemas.android.com/apk/res/android"

    private fun fullComponentName(raw: String): String =
        if (raw.startsWith(".")) "com.wangzan101.mika$raw" else raw

    @Test
    fun fileProvidersGrantUriPermissionsAndStayPrivate() {
        val manifest = File("src/main/AndroidManifest.xml")
        assertTrue("manifest not found from ${File(".").absolutePath}", manifest.exists())
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(manifest)
        val providers = doc.getElementsByTagName("provider")
        var fileProviders = 0
        for (i in 0 until providers.length) {
            val p = providers.item(i) as Element
            val metas = p.getElementsByTagName("meta-data")
            val isFileProvider = (0 until metas.length).any {
                (metas.item(it) as Element).getAttributeNS(androidNs, "name") == "android.support.FILE_PROVIDER_PATHS"
            }
            if (!isFileProvider) continue
            fileProviders++
            val name = p.getAttributeNS(androidNs, "name")
            // FileProvider.attachInfo 在 grantUriPermissions 不为 true 时抛 SecurityException,进程起不来
            assertEquals("$name grantUriPermissions", "true", p.getAttributeNS(androidNs, "grantUriPermissions"))
            assertEquals("$name exported", "false", p.getAttributeNS(androidNs, "exported"))
        }
        assertTrue("expected at least one FileProvider", fileProviders > 0)
    }

    @Test
    fun helperHandoffProviderIsOnlyReachableWithShellLevelPermission() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val providers = doc.getElementsByTagName("provider")
        val p = (0 until providers.length).map { providers.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.shell.HelperHandoffProvider" }
        // shell 要能调到它;普通 App 没有这个权限,连 provider 都拿不到
        assertEquals("true", p.getAttributeNS(androidNs, "exported"))
        assertEquals("android.permission.INTERACT_ACROSS_USERS_FULL", p.getAttributeNS(androidNs, "permission"))
        assertEquals("\${applicationId}.helper", p.getAttributeNS(androidNs, "authorities"))
    }

    /** 电脑连接的服务对外开放只为让 adb shell 起它:普通 App 没有这个权限,起不了、也开关不了 USB 方式。 */
    @Test
    fun computerAccessServiceIsOnlyStartableWithShellLevelPermission() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val services = doc.getElementsByTagName("service")
        val s = (0 until services.length).map { services.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.mcp.McpService" }
        assertEquals("true", s.getAttributeNS(androidNs, "exported"))
        assertEquals("android.permission.INTERACT_ACROSS_USERS_FULL", s.getAttributeNS(androidNs, "permission"))
        assertEquals("specialUse", s.getAttributeNS(androidNs, "foregroundServiceType"))
        // 没有 intent-filter:只能按组件名显式起
        assertEquals(0, s.getElementsByTagName("intent-filter").length)
    }

    /** 清单里登记的组件都按全名找得到类(库模块不覆写宿主的 application 属性,所有组件均写全类名)。 */
    @Test
    fun everyComponentClassExists() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val names = listOf("activity", "service", "receiver", "provider").flatMap { tag ->
            val nodes = doc.getElementsByTagName(tag)
            (0 until nodes.length).map { (nodes.item(it) as Element).getAttributeNS(androidNs, "name") }
        }
        assertTrue(names.size > 5)
        for (n in names) {
            assertFalse("library manifest component should use FQCN, got $n", n.startsWith("."))
            val full = fullComponentName(n)
            assertTrue("$full is not a class", runCatching { Class.forName(full, false, javaClass.classLoader) }.isSuccess)
        }
    }

    /** 库清单不得声明 LAUNCHER、REQUEST_INSTALL_PACKAGES 或覆盖宿主 <application> 属性。 */
    @Test
    fun libraryManifestMergesCleanlyIntoHostApplication() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val appElement = doc.getElementsByTagName("application").item(0) as Element
        for (attr in listOf("name", "icon", "roundIcon", "label", "theme", "allowBackup")) {
            assertFalse("library <application> must not set android:$attr", appElement.hasAttributeNS(androidNs, attr))
        }
        val permissions = doc.getElementsByTagName("uses-permission")
        val permissionNames = (0 until permissions.length).map {
            (permissions.item(it) as Element).getAttributeNS(androidNs, "name")
        }
        assertFalse(
            "REQUEST_INSTALL_PACKAGES must not be in library main manifest",
            "android.permission.REQUEST_INSTALL_PACKAGES" in permissionNames,
        )
        val categories = appElement.getElementsByTagName("category")
        val categoryNames = (0 until categories.length).map {
            (categories.item(it) as Element).getAttributeNS(androidNs, "name")
        }
        assertFalse(
            "library manifest must not register a second LAUNCHER icon",
            "android.intent.category.LAUNCHER" in categoryNames,
        )
        val receivers = doc.getElementsByTagName("receiver")
        val scheduleReceiver = (0 until receivers.length).map { receivers.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.task.ExternalScheduleReceiver" }
        val scheduleActions = (0 until scheduleReceiver.getElementsByTagName("action").length).map {
            (scheduleReceiver.getElementsByTagName("action").item(it) as Element).getAttributeNS(androidNs, "name")
        }.toSet()
        assertTrue("com.ombhrum.fabushi.SCHEDULE" in scheduleActions)
        assertTrue("com.wangzan101.mika.SCHEDULE" in scheduleActions)
    }

    /**
     * 测试 APK 清单若存在,里面登记的组件需要同名 Java 源文件。
     */
    @Test
    fun testApkComponentsAreJavaClassesWithMatchingNames() {
        val androidTestManifest = File("src/androidTest/AndroidManifest.xml")
        if (!androidTestManifest.isFile) return
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(androidTestManifest)
        val names = listOf("activity", "service", "receiver", "provider").flatMap { tag ->
            val nodes = doc.getElementsByTagName(tag)
            (0 until nodes.length).map { (nodes.item(it) as Element).getAttributeNS(androidNs, "name") }
        }
        assertTrue(names.isNotEmpty())
        for (n in names) {
            val path = "src/androidTest/java/${n.replace('.', '/')}"
            assertTrue("$n has no Java source at $path.java", File("$path.java").isFile)
        }
    }

    @Test
    fun cameraIsDeclaredAsOptionalFeature() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val features = doc.getElementsByTagName("uses-feature")
        val camera = (0 until features.length).map { features.item(it) as Element }
            .singleOrNull { it.getAttributeNS(androidNs, "name") == "android.hardware.camera" }
        assertTrue("uses-feature android.hardware.camera is missing", camera != null)
        assertEquals("false", camera!!.getAttributeNS(androidNs, "required"))
    }

    @Test
    fun locationIsDeclaredAsAnOptionalFeature() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val nodes = doc.getElementsByTagName("uses-feature")
        val features = (0 until nodes.length).map { nodes.item(it) as Element }
            .associateBy { it.getAttributeNS(androidNs, "name") }
        for (name in listOf("android.hardware.location", "android.hardware.location.gps")) {
            val feature = features[name]
            assertTrue("uses-feature $name is missing", feature != null)
            assertEquals(name, "false", feature!!.getAttributeNS(androidNs, "required"))
        }
    }

    @Test
    fun selfCheckTargetIsOnlyStartableWithShellLevelPermission() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val activities = doc.getElementsByTagName("activity")
        val a = (0 until activities.length).map { activities.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.selfcheck.SelfCheckTargetActivity" }
        assertEquals("true", a.getAttributeNS(androidNs, "exported"))
        assertEquals("android.permission.INTERACT_ACROSS_USERS_FULL", a.getAttributeNS(androidNs, "permission"))
        assertEquals(0, a.getElementsByTagName("intent-filter").length)
        assertEquals(com.wangzan101.mika.cap.phone.shell.SystemIntents.OWN_PAGE_CLASS, fullComponentName(a.getAttributeNS(androidNs, "name")))
        assertEquals("true", a.getAttributeNS(androidNs, "excludeFromRecents"))
        assertTrue(a.hasAttributeNS(androidNs, "taskAffinity"))
        assertEquals("", a.getAttributeNS(androidNs, "taskAffinity"))
    }

    @Test fun capsuleTileIsBoundOnlyByAndroidQuickSettings() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val services = doc.getElementsByTagName("service")
        val tile = (0 until services.length).map { services.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.capsule.CapsuleTileService" }
        assertEquals("false", tile.getAttributeNS(androidNs, "enabled"))
        assertEquals("true", tile.getAttributeNS(androidNs, "exported"))
        assertEquals("@string/capsule_tile_label", tile.getAttributeNS(androidNs, "label"))
        assertEquals("@drawable/ic_notification", tile.getAttributeNS(androidNs, "icon"))
        assertEquals("android.permission.BIND_QUICK_SETTINGS_TILE", tile.getAttributeNS(androidNs, "permission"))
        val actions = tile.getElementsByTagName("action")
        assertEquals(1, actions.length)
        assertEquals("android.service.quicksettings.action.QS_TILE",
            (actions.item(0) as Element).getAttributeNS(androidNs, "name"))
    }

    @Test fun capsuleTileRuntimeKeepsTheSameSystemSafeIcon() {
        val service = File("src/main/java/com/wangzan101/mika/capsule/CapsuleTileService.kt").readText()
        assertTrue(service.contains("Icon.createWithResource(this, R.drawable.ic_notification)"))
        assertTrue(!service.contains("Icon.createWithResource(this, R.mipmap.ic_launcher)"))
    }

    @Test fun capsuleTileLongPressOpensMikaInsteadOfAppDetails() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val activities = doc.getElementsByTagName("activity")
        val main = (0 until activities.length).map { activities.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.ui.MainActivity" }
        val filters = main.getElementsByTagName("intent-filter")
        val preferences = (0 until filters.length).map { filters.item(it) as Element }.single { filter ->
            val actions = filter.getElementsByTagName("action")
            (0 until actions.length).any {
                (actions.item(it) as Element).getAttributeNS(androidNs, "name") ==
                    "android.service.quicksettings.action.QS_TILE_PREFERENCES"
            }
        }
        val categories = preferences.getElementsByTagName("category")
        assertTrue((0 until categories.length).any {
            (categories.item(it) as Element).getAttributeNS(androidNs, "name") == "android.intent.category.DEFAULT"
        })
    }

    /** 内置助手进程按这个全名启动。 */
    @Test
    fun helperEntryClassExists() {
        val c = Class.forName(com.wangzan101.mika.core.shell.HelperHandoff.HELPER_CLASS, false, javaClass.classLoader)
        assertTrue(c.methods.any { it.name == "main" && java.lang.reflect.Modifier.isStatic(it.modifiers) })
    }

    @Test
    fun reportFilesProviderAndViewerSoftInputAreConfigured() {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(File("src/main/AndroidManifest.xml"))
        val providers = doc.getElementsByTagName("provider")
        val reportProvider = (0 until providers.length).map { providers.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.ui.ReportFilesProvider" }
        assertEquals("com.wangzan101.mika.ui.ReportFilesProvider", reportProvider.getAttributeNS(androidNs, "name"))
        assertEquals("\${applicationId}.reportfiles", reportProvider.getAttributeNS(androidNs, "authorities"))
        assertEquals("false", reportProvider.getAttributeNS(androidNs, "exported"))
        assertEquals("true", reportProvider.getAttributeNS(androidNs, "grantUriPermissions"))
        val metas = reportProvider.getElementsByTagName("meta-data")
        val pathMeta = (0 until metas.length).map { metas.item(it) as Element }
            .single { it.getAttributeNS(androidNs, "name") == "android.support.FILE_PROVIDER_PATHS" }
        assertEquals("@xml/report_files", pathMeta.getAttributeNS(androidNs, "resource"))
        assertTrue("res/xml/report_files.xml must exist", File("src/main/res/xml/report_files.xml").isFile)

        val activities = doc.getElementsByTagName("activity")
        val viewer = (0 until activities.length).map { activities.item(it) as Element }
            .single { fullComponentName(it.getAttributeNS(androidNs, "name")) == "com.wangzan101.mika.ui.viewer.FileViewerActivity" }
        assertEquals("adjustResize", viewer.getAttributeNS(androidNs, "windowSoftInputMode"))
    }
}
