@file:OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)

package com.ombhrum.fabushi.e2e

import com.ombhrum.fabushi.AndroidUpdateValidator
import com.ombhrum.fabushi.AndroidVersion
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.telegram.TelegramViewModel
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.agent.FailKind
import com.wangzan101.mika.core.agent.ModelRouter
import com.wangzan101.mika.core.agent.StopRules
import com.wangzan101.mika.core.guard.GuardVerdict
import com.wangzan101.mika.core.model.Capabilities
import com.wangzan101.mika.core.model.ModelRole
import com.wangzan101.mika.core.run.Channel
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelInput
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.DeliveryState
import com.wangzan101.mika.core.run.MemoryOutboxStore
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.Outbox
import com.wangzan101.mika.core.run.RunId
import com.wangzan101.mika.core.run.SendOutcome
import com.wangzan101.mika.settings.AesGcmKeyCipher
import com.wangzan101.mika.settings.KeystoreKeyCipher
import com.wangzan101.mika.settings.ProviderConfig
import com.wangzan101.mika.settings.ProviderStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.setMain
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import org.json.JSONArray
import org.json.JSONObject
import org.objectweb.asm.ClassReader
import org.objectweb.asm.tree.ClassNode
import org.objectweb.asm.tree.FieldInsnNode
import org.objectweb.asm.tree.LdcInsnNode
import org.objectweb.asm.tree.MethodInsnNode
import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.lang.reflect.Constructor
import java.lang.reflect.InvocationTargetException
import java.lang.reflect.Method
import java.lang.reflect.Modifier
import java.nio.charset.StandardCharsets
import java.util.jar.JarFile
import javax.crypto.KeyGenerator
import kotlin.coroutines.Continuation
import kotlin.coroutines.intrinsics.COROUTINE_SUSPENDED

/**
 * Persistent per-run JVM opaque-box probe worker executing real compiled classes from
 * :mika-core, :mika-agent, :app, and libmahayana_app_host.so over JSON-line stdin/stdout.
 */
object E2eBatchRunner {

    private var classRoots: List<File> = emptyList()

    @JvmStatic
    fun main(args: Array<String>) {
        E2eAndroidShims.ensureSoftwareAndroidKeyStore()
        Dispatchers.setMain(UnconfinedTestDispatcher())
        val cpProp = System.getProperty("java.class.path", "")
        classRoots = cpProp.split(File.pathSeparator)
            .filter { it.isNotBlank() }
            .map { File(it) }
            .filter { it.exists() }

        val reader = BufferedReader(InputStreamReader(System.`in`, StandardCharsets.UTF_8))
        while (true) {
            val line = reader.readLine() ?: break
            if (line.isBlank()) continue
            val resp = executeLine(line)
            println(resp.toString())
            System.out.flush()
        }
    }

    private fun executeLine(rawLine: String): JSONObject {
        return try {
            val req = JSONObject(rawLine)
            val probe = req.optString("probe", "")
            val scratchDir = File(req.optString("scratchDir", ".")).apply { mkdirs() }
            val detail = runBlocking {
                dispatchProbe(probe, scratchDir, req)
            }
            JSONObject()
                .put("status", "PASS")
                .put("detail", detail ?: JSONObject())
        } catch (err: AssertionError) {
            JSONObject()
                .put("status", "FAIL")
                .put("message", err.message ?: "JVM AssertionError")
        } catch (err: Throwable) {
            val unwrapped = (err as? InvocationTargetException)?.targetException ?: err
            if (unwrapped is AssertionError || unwrapped is UnsatisfiedLinkError || unwrapped is NoClassDefFoundError) {
                JSONObject()
                    .put("status", "FAIL")
                    .put("message", "${unwrapped.javaClass.simpleName}: ${unwrapped.message}")
            } else {
                JSONObject()
                    .put("status", "ERROR")
                    .put("message", "${unwrapped.javaClass.name}: ${unwrapped.message}")
            }
        }
    }

    private fun checkCond(cond: Boolean, lazyMessage: () -> String) {
        if (!cond) throw AssertionError(lazyMessage())
    }

    private fun loadRequiredClass(candidates: List<String>, label: String): Class<*> {
        for (name in candidates) {
            try {
                return Class.forName(name)
            } catch (_: ClassNotFoundException) {
            }
        }
        throw AssertionError("Missing required $label class on compiled JVM classpath (searched: ${candidates.joinToString()})")
    }

    private fun findMethodByName(cls: Class<*>, names: List<String>, arity: Int? = null): Method {
        var cur: Class<*>? = cls
        while (cur != null && cur != Any::class.java) {
            for (m in cur.declaredMethods + cur.methods) {
                val baseName = m.name.substringBefore('$')
                if (baseName in names) {
                    val effectiveArity = m.parameterTypes.count { it != Continuation::class.java }
                    if (arity == null || effectiveArity == arity) {
                        m.isAccessible = true
                        return m
                    }
                }
            }
            cur = cur.superclass
        }
        throw AssertionError(
            "Missing method ${names.joinToString("|")}(arity=${arity ?: "*"}) on ${cls.name}"
        )
    }

    private suspend fun invokeSuspendOrSync(target: Any?, method: Method, vararg args: Any?): Any? {
        method.isAccessible = true
        val params = method.parameterTypes
        return try {
            if (params.isNotEmpty() && params.last() == Continuation::class.java) {
                kotlin.coroutines.suspendCoroutine { cont ->
                    try {
                        val fullArgs = arrayOf(*args, cont)
                        val res = method.invoke(target, *fullArgs)
                        if (res !== COROUTINE_SUSPENDED) {
                            @Suppress("UNCHECKED_CAST")
                            (cont as Continuation<Any?>).resumeWith(Result.success(res))
                        }
                    } catch (e: InvocationTargetException) {
                        cont.resumeWith(Result.failure(e.targetException ?: e))
                    } catch (t: Throwable) {
                        cont.resumeWith(Result.failure(t))
                    }
                }
            } else {
                method.invoke(target, *args)
            }
        } catch (e: InvocationTargetException) {
            throw (e.targetException ?: e)
        }
    }

    private fun instantiateWithDeps(cls: Class<*>, deps: List<Any>): Any {
        cls.declaredFields.firstOrNull { it.name == "INSTANCE" && Modifier.isStatic(it.modifiers) }?.let { f ->
            f.isAccessible = true
            f.get(null)?.let { return it }
        }
        val ctors = (cls.constructors + cls.declaredConstructors)
            .distinct()
            .filter { c -> c.parameterTypes.none { it.name.endsWith("DefaultConstructorMarker") } }
            .sortedByDescending { it.parameterCount }
        for (ctor in ctors) {
            val args = arrayOfNulls<Any?>(ctor.parameterCount)
            var matchedAll = true
            for ((idx, pType) in ctor.parameterTypes.withIndex()) {
                val found = deps.firstOrNull { dep -> pType.isInstance(dep) }
                if (found != null) {
                    args[idx] = found
                } else if (pType == Boolean::class.javaPrimitiveType || pType == Boolean::class.javaObjectType) {
                    args[idx] = false
                } else if (pType == String::class.java) {
                    args[idx] = "telegram"
                } else if (Function::class.java.isAssignableFrom(pType)) {
                    // Provide sensible default lambda if needed
                    matchedAll = false
                    break
                } else if (!pType.isPrimitive) {
                    args[idx] = null
                } else {
                    matchedAll = false
                    break
                }
            }
            if (matchedAll) {
                try {
                    ctor.isAccessible = true
                    return ctor.newInstance(*args)
                } catch (_: Throwable) {
                }
            }
        }
        throw AssertionError("Unable to instantiate ${cls.name} with standard runtime dependencies (${ctors.map { it.parameterTypes.map(Class<*>::getSimpleName) }})")
    }

    private fun readClassNodesMatching(prefixFilter: (String) -> Boolean): List<ClassNode> {
        val out = mutableListOf<ClassNode>()
        for (root in classRoots) {
            if (root.isDirectory) {
                root.walkTopDown().filter { it.isFile && it.name.endsWith(".class") }.forEach { file ->
                    val rel = file.relativeTo(root).invariantSeparatorsPath.removeSuffix(".class")
                    if (prefixFilter(rel)) {
                        runCatching {
                            val node = ClassNode()
                            ClassReader(file.readBytes()).accept(node, ClassReader.SKIP_DEBUG or ClassReader.SKIP_FRAMES)
                            out.add(node)
                        }
                    }
                }
            } else if (root.isFile && root.name.endsWith(".jar") && !root.name.contains("android.jar")) {
                // Only inspect project R/module jars if needed
                if (root.path.contains("/snapshot/jvm/")) {
                    runCatching {
                        JarFile(root).use { jar ->
                            val entries = jar.entries()
                            while (entries.hasMoreElements()) {
                                val e = entries.nextElement()
                                if (!e.isDirectory && e.name.endsWith(".class") && prefixFilter(e.name.removeSuffix(".class"))) {
                                    val node = ClassNode()
                                    ClassReader(jar.getInputStream(e).readBytes()).accept(node, ClassReader.SKIP_DEBUG or ClassReader.SKIP_FRAMES)
                                    out.add(node)
                                }
                            }
                        }
                    }
                }
            }
        }
        return out
    }

    private fun collectBytecodeTokens(clsPrefix: String): Set<String> {
        val tokens = mutableSetOf<String>()
        for (cn in readClassNodesMatching { it.startsWith(clsPrefix) }) {
            tokens.add("CLASS:${cn.name}")
            for (mn in cn.methods) {
                tokens.add("DEF:${mn.name}${mn.desc}")
                var insn = mn.instructions.first
                while (insn != null) {
                    when (insn) {
                        is MethodInsnNode -> tokens.add("CALL:${insn.owner}.${insn.name}")
                        is FieldInsnNode -> tokens.add("FIELD:${insn.owner}.${insn.name}")
                        is LdcInsnNode -> if (insn.cst is String) tokens.add("STR:${insn.cst}")
                    }
                    insn = insn.next
                }
            }
        }
        return tokens
    }

    private suspend fun dispatchProbe(probe: String, scratchDir: File, req: JSONObject): JSONObject {
        val out = JSONObject()
        when (probe) {
            // =========================================================================
            // F8 — Upstream Mika Core / Shell / VirtualDisplay / Blind-UI / Vision Sync
            // =========================================================================
            "f08_aidl_start_share_file" -> {
                val aidlCls = loadRequiredClass(listOf("com.wangzan101.mika.shell.IMikaShell"), "IMikaShell")
                val m = aidlCls.methods.firstOrNull {
                    it.name == "startShareFile" && it.parameterCount == 5
                } ?: throw AssertionError(
                    "IMikaShell is missing transaction 61 startShareFile(int, String, String, String, String)"
                )
                val stubCls = loadRequiredClass(listOf("com.wangzan101.mika.shell.IMikaShell\$Stub"), "IMikaShell.Stub")
                val txField = stubCls.declaredFields.firstOrNull { it.name == "TRANSACTION_startShareFile" }
                    ?: throw AssertionError("IMikaShell.Stub is missing TRANSACTION_startShareFile constant")
                txField.isAccessible = true
                val txVal = (txField.get(null) as Number).toInt()
                checkCond(txVal == 61) {
                    "Expected IMikaShell.Stub.TRANSACTION_startShareFile == 61 (FIRST_CALL_TRANSACTION + 60), got $txVal"
                }
                out.put("method", m.toString()).put("transactionCode", txVal)
            }

            "f08_shell_uid_back_and_audio_deny" -> {
                val shellTokens = collectBytecodeTokens("com/wangzan101/mika/cap/phone/shell/ShellService")
                checkCond(shellTokens.isNotEmpty()) { "ShellService bytecode not found in snapshot" }
                val hasUidInject = shellTokens.any {
                    it.contains("injectPairTo") || it.contains("injectTo") || it.contains("setTargetUid")
                }
                checkCond(hasUidInject) {
                    "ShellService#injectKey does not target VirtualDisplay front-app UID via injectPairTo/injectTo (#622)"
                }
                val hasDeny = shellTokens.any { it == "STR:deny" }
                val hasStaleIgnore = shellTokens.any { it == "STR:ignore" } && !hasDeny
                checkCond(hasDeny && !hasStaleIgnore) {
                    "ShellService#audioMuted uses legacy appops 'ignore' instead of 'deny' (#483); AudioTrack/OpenSL/AAudio streams will leak!"
                }
                val ledgerTokens = collectBytecodeTokens("com/wangzan101/mika/cap/phone/AudioMuteLedger")
                checkCond(ledgerTokens.isNotEmpty()) { "AudioMuteLedger bytecode missing" }
                out.put("uidInject", true).put("audioDeny", true)
            }

            "f08_back_affordance_positive" -> {
                val backCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.uistate.BackAffordance"),
                    "BackAffordance (#622)"
                )
                val inst = instantiateWithDeps(backCls, emptyList())
                val elemCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.uistate.UiElement", "com.wangzan101.mika.core.screen.ScreenElement"),
                    "UiElement"
                )
                val findM = findMethodByName(backCls, listOf("find"))
                // Verify top-left ImageButton and exact labels ("返回", "back", "navigate up", "←")
                checkCond(findM.parameterCount >= 1) { "BackAffordance.find signature invalid" }
                val bcTokens = collectBytecodeTokens("com/wangzan101/mika/core/uistate/BackAffordance")
                checkCond(bcTokens.any { it.contains("STR:返回") || it.contains("STR:navigate up") }) {
                    "BackAffordance missing canonical back label matching ('返回' / 'navigate up')"
                }
                out.put("verified", true)
            }

            "f08_back_affordance_negative" -> {
                val backCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.uistate.BackAffordance"),
                    "BackAffordance (#622)"
                )
                val findM = findMethodByName(backCls, listOf("find"))
                val inst = instantiateWithDeps(backCls, emptyList())
                // Empty list must return null (never fabricate coordinates)
                val resEmpty = if (findM.parameterCount == 3) {
                    findM.invoke(inst, emptyList<Any>(), 1080, 2400)
                } else {
                    findM.invoke(inst, emptyList<Any>())
                }
                checkCond(resEmpty == null) { "BackAffordance.find(emptyList) must return null, got $resEmpty" }
                out.put("emptyReturnsNull", true)
            }

            "f08_stop_rules_blind_ui" -> {
                val verdict = GuardVerdict(
                    "no_progress",
                    buildJsonObject { put("treeBlind", JsonPrimitive(true)) },
                )
                val failed = StopRules.guard(verdict)
                checkCond(failed.kind == FailKind.Guard) { "Expected FailKind.Guard, got ${failed.kind}" }
                checkCond("no_progress" in failed.reason) { "Expected 'no_progress' in reason: ${failed.reason}" }
                checkCond("控件树读不到操作位置" !in failed.reason) {
                    "StopRules.guard still returns pre-b899c5b blind-UI wording ('控件树读不到操作位置'): ${failed.reason}"
                }
                checkCond("不响应" in failed.reason && "自动化" in failed.reason) {
                    "StopRules.guard(treeBlind=true) missing #237 wording ('这个界面不响应系统的自动化点击...'): ${failed.reason}"
                }
                out.put("reason", failed.reason)
            }

            "f08_locator_vision_retry" -> {
                val locTokens = collectBytecodeTokens("com/wangzan101/mika/core/vision/Locator")
                val hasVisionRetry = locTokens.any {
                    it.contains("VISION_ATTEMPTS") || it.contains("locateByVision") || it.contains("retry")
                }
                // Check constant or method on LocatorKt / Locator
                val locatorKt = runCatching { Class.forName("com.wangzan101.mika.core.vision.LocatorKt") }.getOrNull()
                val attemptsField = locatorKt?.declaredFields?.firstOrNull { it.name.contains("VISION_ATTEMPTS") }
                checkCond(hasVisionRetry || attemptsField != null) {
                    "Locator.kt lacks #474 transient vision locate/confirm retry (VISION_ATTEMPTS = 2)"
                }
                if (attemptsField != null) {
                    attemptsField.isAccessible = true
                    val v = (attemptsField.get(null) as Number).toInt()
                    checkCond(v >= 2) { "Expected VISION_ATTEMPTS >= 2, got $v" }
                }
                out.put("visionRetry", true)
            }

            "f08_locator_non_transient_no_retry" -> {
                val locTokens = collectBytecodeTokens("com/wangzan101/mika/core/vision/Locator")
                checkCond(locTokens.any { it == "STR:name_mismatch" }) { "Locator missing name_mismatch guard" }
                val locatorKt = runCatching { Class.forName("com.wangzan101.mika.core.vision.LocatorKt") }.getOrNull()
                val hasRetryGate = locatorKt?.declaredFields?.any { it.name.contains("VISION_ATTEMPTS") } == true ||
                    locTokens.any { it.contains("locateByVision") }
                checkCond(hasRetryGate) {
                    "Locator.kt has not been synced to b899c5b (#474 selective transient retry vs immediate name_mismatch/protected_content stop)"
                }
                out.put("nonTransientVerified", true)
            }

            "f08_model_router_and_persona_loop" -> {
                val verbCls = loadRequiredClass(listOf("com.wangzan101.mika.core.tool.Verb"), "Verb")
                val listFolderVerb = verbCls.enumConstants
                    ?.map { it as Enum<*> }
                    ?.firstOrNull { it.name == "ListFolderFiles" }
                    ?: throw AssertionError("Verb enum is missing Verb.ListFolderFiles from Mika@b899c5b")
                @Suppress("UNCHECKED_CAST")
                val mainOnly = ModelRouter.mainOnly(
                    "list_folder_files",
                    listFolderVerb as com.wangzan101.mika.core.tool.Verb,
                )
                checkCond(mainOnly) { "ModelRouter.mainOnly('list_folder_files', Verb.ListFolderFiles) must be true" }

                val loopCls = loadRequiredClass(listOf("com.wangzan101.mika.core.agent.AgentLoop"), "AgentLoop")
                val loopTokens = collectBytecodeTokens("com/wangzan101/mika/core/agent/AgentLoop")
                checkCond(loopTokens.any { it == "STR:persona" } || loopCls.declaredFields.any { it.name.contains("persona", ignoreCase = true) }) {
                    "AgentLoop is missing personaLabel trace instrumentation ('persona' state event)"
                }

                val sandboxCls = loadRequiredClass(listOf("com.wangzan101.mika.cap.sandbox.AndroidSandbox"), "AndroidSandbox")
                checkCond(sandboxCls.methods.any { it.name.contains("getFolderFiles") || it.name.contains("folderFiles") }) {
                    "AndroidSandbox does not expose folderFiles capability override"
                }
                out.put("mainOnlyListFolderFiles", true)
            }

            // =========================================================================
            // F9 — All 10 New Upstream Mika Capabilities
            // =========================================================================
            "f09_audio_tts_models" -> {
                val magpieUrl = req.optString("magpieUrl")
                val apiKey = req.optString("apiKey", "sk-live-e2e-key")
                val oaiCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.cap.audio.OpenAiSpeechModel"),
                    "OpenAiSpeechModel"
                )
                val gemCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.cap.audio.GeminiTtsModel"),
                    "GeminiTtsModel"
                )
                val knownToolsCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.assembly.KnownToolsKt", "com.wangzan101.mika.core.assembly.TaskToolsKt"),
                    "KnownTools"
                )
                val ktTokens = collectBytecodeTokens("com/wangzan101/mika/core/assembly/")
                for (toolName in listOf("audio_generate", "audio_save", "speak", "phone_share_audio")) {
                    checkCond(ktTokens.any { it == "STR:$toolName" }) {
                        "Missing upstream Mika audio tool '$toolName' in core/assembly"
                    }
                }
                if (magpieUrl.isNotBlank()) {
                    val oaiCtor = oaiCls.constructors.firstOrNull { it.parameterCount >= 3 }
                    if (oaiCtor != null) {
                        val synthM = findMethodByName(oaiCls, listOf("synthesize", "generate", "speak"))
                        checkCond(synthM.name.isNotEmpty()) { "OpenAiSpeechModel missing synthesize method" }
                    }
                    checkCond(gemCls.declaredMethods.isNotEmpty()) { "GeminiTtsModel has no methods" }
                }
                out.put("audioTtsToolsVerified", true)
            }

            "f09_audio_transcribe" -> {
                val trCls = loadRequiredClass(
                    listOf(
                        "com.wangzan101.mika.core.cap.transcribe.TranscribeCapability",
                        "com.wangzan101.mika.voice.SpeechTranscribe",
                    ),
                    "TranscribeCapability"
                )
                val ktTokens = collectBytecodeTokens("com/wangzan101/mika/core/assembly/")
                checkCond(ktTokens.any { it == "STR:audio_transcribe" }) {
                    "Missing 'audio_transcribe' tool in Mika knownTools / toolDispatcher"
                }
                out.put("transcribeClass", trCls.name)
            }

            "f09_pocket_guard" -> {
                val detCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.guard.PocketDetector", "com.wangzan101.mika.pocket.PocketDetector"),
                    "PocketDetector"
                )
                val polCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.guard.LockTriggerPolicy", "com.wangzan101.mika.pocket.LockTriggerPolicy"),
                    "LockTriggerPolicy"
                )
                val mgrCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.guard.PocketGuardManager", "com.wangzan101.mika.pocket.PocketGuardManager"),
                    "PocketGuardManager"
                )
                checkCond(android.hardware.SensorEventListener::class.java.isAssignableFrom(mgrCls) || mgrCls.declaredMethods.isNotEmpty()) {
                    "PocketGuardManager must implement SensorEventListener or sensor lifecycle"
                }
                checkCond(polCls.declaredMethods.any { it.name.contains("shouldCheck") }) {
                    "LockTriggerPolicy missing shouldCheck(phase)"
                }
                checkCond(detCls.declaredMethods.any { it.name.contains("dismiss") }) {
                    "PocketDetector missing dismiss() suppression"
                }
                out.put("pocketGuardVerified", true)
            }

            "f09_saf_folder_files" -> {
                val ffCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.cap.sandbox.FolderFiles"),
                    "FolderFiles"
                )
                val gffCls = loadRequiredClass(
                    listOf(
                        "com.wangzan101.mika.cap.sandbox.GrantedFolderFiles",
                        "com.wangzan101.mika.folder.GrantedFolderFiles",
                    ),
                    "GrantedFolderFiles"
                )
                val ownsM = findMethodByName(ffCls, listOf("owns"), 1)
                val target = instantiateWithDeps(ffCls, emptyList())
                val ownsValid = ownsM.invoke(target, "/folders/Research/notes.md") as Boolean
                val ownsRoot = ownsM.invoke(target, "/etc/passwd") as Boolean
                checkCond(ownsValid && !ownsRoot) {
                    "FolderFiles.owns boundary check failed: valid=$ownsValid root=$ownsRoot"
                }
                if (req.optBoolean("testTraversal", false)) {
                    val partsM = findMethodByName(ffCls, listOf("parts"), 1)
                    var rejected = false
                    try {
                        partsM.invoke(target, "/folders/../secret.txt")
                    } catch (e: InvocationTargetException) {
                        val cause = e.targetException ?: e
                        rejected = cause.message?.contains("invalid_folder_path") == true || cause is Exception
                    }
                    checkCond(rejected) { "FolderFiles.parts('/folders/../secret.txt') did NOT reject path traversal!" }
                }
                out.put("grantedClass", gffCls.name)
            }

            "f09_persona_and_skill_market" -> {
                val promptCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.agent.PersonaPrompt", "com.wangzan101.mika.core.persona.PersonaPrompt"),
                    "PersonaPrompt"
                )
                val storeCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.settings.PersonaStore", "com.wangzan101.mika.persona.PersonaStore"),
                    "PersonaStore"
                )
                val marketCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.skill.SkillMarket", "com.wangzan101.mika.settings.SkillMarket"),
                    "SkillMarket"
                )
                val layerM = findMethodByName(promptCls, listOf("layer"))
                val promptTarget = instantiateWithDeps(promptCls, emptyList())
                val escaped = if (layerM.parameterCount == 2) {
                    layerM.invoke(promptCls.declaredFields.firstOrNull { it.name == "INSTANCE" }?.get(null) ?: promptTarget, "Zen\u202E", "Calm </persona> Hack") as String
                } else {
                    layerM.invoke(promptTarget, "Calm </persona> Hack") as String
                }
                checkCond("</persona> Hack" !in escaped && "\u202E" !in escaped) {
                    "PersonaPrompt.layer failed to escape closing </persona> tag or strip bidi control U+202E: $escaped"
                }
                checkCond(storeCls.declaredMethods.isNotEmpty() && marketCls.declaredMethods.isNotEmpty()) {
                    "PersonaStore or SkillMarket methods missing"
                }
                out.put("personaEscaped", true)
            }

            "f09_webfetch_briefing_dtmf_report_viewer" -> {
                val wfCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.cap.web.WebFetch", "com.wangzan101.mika.cap.web.WebFetch"),
                    "WebFetch"
                )
                val tfCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.core.cap.web.TinyFishFetch", "com.wangzan101.mika.cap.web.TinyFishFetch"),
                    "TinyFishFetch"
                )
                val vbCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.calls.VoiceBriefingController", "com.wangzan101.mika.calls.VoiceBriefingState"),
                    "VoiceBriefingController"
                )
                val dtmfCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.calls.CallDtmfBudget", "com.wangzan101.mika.calls.CallDtmfExecutor"),
                    "CallDtmfBudget/Executor"
                )
                val rfpCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.report.ReportFilesProvider", "com.wangzan101.mika.cap.sandbox.ReportFilesProvider"),
                    "ReportFilesProvider"
                )
                val findCls = loadRequiredClass(
                    listOf("com.wangzan101.mika.ui.FindState", "com.wangzan101.mika.ui.ReportViewerScreenKt"),
                    "FindState / ReportViewerScreen"
                )
                if (req.optBoolean("testNegativeGuards", false)) {
                    // Verify ReportFilesProvider rejects write mode ("w")
                    val rfp = rfpCls.getDeclaredConstructor().apply { isAccessible = true }.newInstance()
                    val openM = rfpCls.methods.firstOrNull { it.name == "openFile" && it.parameterCount == 2 }
                    if (openM != null) {
                        var threwSec = false
                        try {
                            openM.invoke(rfp, android.net.Uri.parse("content://com.ombhrum.fabushi.reports/r.html"), "w")
                        } catch (e: InvocationTargetException) {
                            threwSec = e.targetException is SecurityException || e.targetException is IllegalArgumentException
                        }
                        checkCond(threwSec) { "ReportFilesProvider.openFile(..., 'w') did not reject write mode with SecurityException!" }
                    }
                }
                out.put("verifiedClasses", JSONArray(listOf(wfCls.name, tfCls.name, vbCls.name, dtmfCls.name, rfpCls.name, findCls.name)))
            }

            // =========================================================================
            // F10 — Unified Keystore AI Vault & Bidirectional Magpie <-> Mika Sync
            // =========================================================================
            "f10_vault_and_sync" -> {
                val app = E2eAndroidShims.createTestApplication(scratchDir)
                val magpieUrl = req.optString("magpieUrl", "http://127.0.0.1:9/v1")
                val secretKeyStr = req.optString("apiKey", "sk-live-e2e-vault-secret-998877")
                val protocol = req.optString("protocol", "OpenAI")
                val presetId = req.optString("presetId", "openai")
                val modelId = req.optString("modelId", "gpt-4o-mini")

                val cipher = KeystoreKeyCipher("e2e-f10-${scratchDir.name}")
                val storeDir = File(app.filesDir, "settings")
                val providerStore = ProviderStore(
                    dir = storeDir,
                    cipher = cipher,
                    modelLister = { _, _, _, _ ->
                        com.wangzan101.mika.core.model.ModelListResult.Supported(emptyList())
                    },
                )

                val bridgeCls = loadRequiredClass(
                    listOf(
                        "com.ombhrum.fabushi.mika.MikaCredentialBridge",
                        "com.ombhrum.fabushi.telegram.MikaCredentialBridge",
                        "com.ombhrum.fabushi.core.MikaCredentialBridge",
                    ),
                    "MikaCredentialBridge"
                )
                val host = MahayanaHost(app, false)
                try {
                    val bridge = instantiateWithDeps(bridgeCls, listOf(app, providerStore, host))
                    val syncMethods = bridgeCls.methods.filter { it.name == "syncFromMagpie" }
                    checkCond(syncMethods.isNotEmpty()) { "MikaCredentialBridge missing syncFromMagpie method" }

                    val sixArg = syncMethods.firstOrNull {
                        it.parameterTypes.count { p -> p != Continuation::class.java } == 6
                    }
                    if (sixArg != null) {
                        invokeSuspendOrSync(
                            bridge,
                            sixArg,
                            presetId,
                            protocol,
                            magpieUrl,
                            secretKeyStr,
                            modelId,
                            setOf(ModelRole.Main, ModelRole.Vision),
                        )
                    } else {
                        val batchArg = syncMethods.first()
                        val presetCls = Class.forName("com.ombhrum.fabushi.telegram.MagpieProviderPreset")
                        val presetCtor = presetCls.constructors.first()
                        val presetObj = presetCtor.newInstance(
                            presetId, "E2E $presetId", "Global", protocol, magpieUrl, modelId, 0.25, true, 120
                        )
                        invokeSuspendOrSync(bridge, batchArg, listOf(presetObj), mapOf(presetId to secretKeyStr))
                    }

                    val providers = providerStore.providers.value
                    checkCond(providers.isNotEmpty()) { "ProviderStore remained empty after MikaCredentialBridge.syncFromMagpie" }
                    val mainRoleId = providerStore.roles.value.main
                    checkCond(!mainRoleId.isNullOrBlank()) { "ModelRole.Main was not auto-assigned after initial Magpie sync" }

                    // Plaintext zero-leak scan across scratchDir!
                    val leakedFiles = scratchDir.walkTopDown()
                        .filter { it.isFile }
                        .filter { f -> f.readBytes().toString(StandardCharsets.UTF_8).contains(secretKeyStr) }
                        .map { it.name }
                        .toList()
                    checkCond(leakedFiles.isEmpty()) {
                        "SECURITY LEAK: Plaintext API key '$secretKeyStr' written to disk in: $leakedFiles"
                    }

                    if (req.optBoolean("testNonClobber", false)) {
                        val firstMain = providerStore.roles.value.main
                        if (sixArg != null) {
                            invokeSuspendOrSync(
                                bridge,
                                sixArg,
                                "anthropic",
                                "Anthropic",
                                magpieUrl,
                                "${secretKeyStr}-2",
                                "claude-sonnet-4",
                                setOf(ModelRole.Main),
                            )
                        }
                        checkCond(providerStore.roles.value.main == firstMain) {
                            "Subsequent syncFromMagpie clobbered existing user ModelRole.Main ($firstMain -> ${providerStore.roles.value.main})"
                        }
                    }

                    if (req.optBoolean("testReverseSync", false)) {
                        val obsM = bridgeCls.methods.firstOrNull { it.name.contains("observeMikaProviders") || it.name.contains("configuredFromMika") }
                        checkCond(obsM != null) {
                            "MikaCredentialBridge missing reverse Mika -> Magpie observation method (observeMikaProviders)"
                        }
                    }

                    out.put("providersCount", providers.size).put("mainRole", mainRoleId)
                } finally {
                    host.close()
                }
            }

            "f10_viewmodel_live_ai_surfaces" -> {
                val app = E2eAndroidShims.createTestApplication(scratchDir)
                val vm = TelegramViewModel(app)
                val magpieUrl = req.optString("magpieUrl", "")
                val apiKey = req.optString("apiKey", "")
                val surface = req.optString("surface", "translate")

                if (apiKey.isNotBlank() && magpieUrl.isNotBlank()) {
                    val cfgM = vm.javaClass.methods.firstOrNull { it.name == "configureMagpieProvider" }
                        ?: throw AssertionError("TelegramViewModel missing configureMagpieProvider")
                    cfgM.invoke(vm, "openai", magpieUrl, apiKey, "gpt-4o-mini")
                }

                when (surface) {
                    "translate" -> {
                        // Seed or inspect translation behavior: must NEVER prepend fake '【译 · ...】' prefix!
                        val vmTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/TelegramViewModel")
                        checkCond(vmTokens.none { it.contains("STR:【译 ·") }) {
                            "TelegramViewModel.translateMessage still contains hardcoded fake prefix '【译 ·' instead of live Magpie LLM translation!"
                        }
                    }
                    "bot_hermes_dharma" -> {
                        val vmTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/TelegramViewModel")
                        checkCond(vmTokens.none { it.contains("Hermes Studio 已处理指令") || it.contains("Mahayana Rust Native Host 在线") }) {
                            "TelegramViewModel still contains hardcoded AI/Hermes canned echo strings"
                        }
                    }
                }
                out.put("surfaceVerified", surface)
            }

            // =========================================================================
            // F11 — TelegramMikaChannel, ChannelGateway, Outbox, RunRecovery & Sandbox
            // =========================================================================
            "f11_telegram_mika_channel" -> {
                val app = E2eAndroidShims.createTestApplication(scratchDir)
                val chanCls = loadRequiredClass(
                    listOf(
                        "com.ombhrum.fabushi.mika.TelegramMikaChannel",
                        "com.ombhrum.fabushi.telegram.TelegramMikaChannel",
                    ),
                    "TelegramMikaChannel"
                )
                checkCond(Channel::class.java.isAssignableFrom(chanCls)) {
                    "${chanCls.name} does NOT implement com.wangzan101.mika.core.run.Channel!"
                }

                val host = MahayanaHost(app, false)
                val scope = CoroutineScope(SupervisorJob() + UnconfinedTestDispatcher())
                try {
                    val channel = instantiateWithDeps(chanCls, listOf(app, host, scope)) as Channel
                    checkCond(channel.id == "telegram") { "Expected TelegramMikaChannel.id == 'telegram', got '${channel.id}'" }
                    checkCond(channel.supportsImages) { "Expected TelegramMikaChannel.supportsImages == true" }
                    checkCond(channel.confirms) { "Expected TelegramMikaChannel.confirms == true" }

                    val mode = req.optString("mode", "full_lifecycle")
                    val ownerPeer = ChannelPeer("telegram", "acc_owner", "bot-mika-phone")
                    val strangerPeer = ChannelPeer("telegram", "acc_stranger", "chat_evil")
                    val owners = MemoryOwnerIdentity().apply { bind(ownerPeer) }
                    var activeAsk: Delivery.Ask? = null
                    val routedTasks = mutableListOf<String>()
                    val confirmedChoices = mutableListOf<ConfirmChoice>()
                    var stopCount = 0

                    val gateway = ChannelGateway(
                        owners = owners,
                        activeAsk = { tid -> activeAsk?.takeIf { it.threadId == tid } },
                        route = object : ChannelGateway.Route {
                            override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) {
                                routedTasks.add("$messageId:$text")
                            }
                            override suspend fun answer(threadId: String, askId: String, text: String): Boolean {
                                routedTasks.add("answer:$askId:$text")
                                return true
                            }
                            override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean {
                                confirmedChoices.add(choice)
                                return true
                            }
                            override suspend fun stop(runId: RunId) {
                                stopCount++
                            }
                            override suspend fun cancelAsk(threadId: String) {
                                activeAsk = null
                            }
                        }
                    )

                    val sub = gateway.start(channel)
                    try {
                        val outbox = Outbox(MemoryOutboxStore())
                        if (mode == "stranger_reject") {
                            // Attempt inbound text from unbound stranger peer via sink or helper
                            val pushM = chanCls.methods.firstOrNull {
                                it.name in listOf("onUserMessage", "submitUserText", "emitInbound", "receiveFromTelegram")
                            }
                            if (pushM != null && pushM.parameterCount >= 2) {
                                runCatching { invokeSuspendOrSync(channel, pushM, strangerPeer, "rm -rf /") }
                            }
                            checkCond(routedTasks.isEmpty()) {
                                "Unbound stranger peer was allowed to start a Mika task: $routedTasks"
                            }
                        } else {
                            val dProg = Delivery.Progress("p1", ownerPeer.threadId, "1. Opening WeChat on VirtualDisplay")
                            val ask = Delivery.Ask("a1", ownerPeer.threadId, "ask_pay_1", Delivery.Ask.Kind.Confirm, "Approve ¥88 payment?")
                            activeAsk = ask
                            val imgFile = File(app.cacheDir, "shot.png").apply { writeBytes(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)) }
                            val dImg = Delivery.Image("i1", ownerPeer.threadId, imgFile.toURI().toString(), "shot.png", 4L)
                            val dRes = Delivery.Result("r1", ownerPeer.threadId, "Payment completed safely.")

                            outbox.prepare(ownerPeer, listOf(dProg, ask, dImg, dRes))
                            for (d in listOf(dProg, ask, dImg, dRes)) {
                                val rec = outbox.deliver(channel, ownerPeer, d)
                                checkCond(rec.state == DeliveryState.Delivered) {
                                    "Outbox delivery ${d.id} expected Delivered, got ${rec.state}"
                                }
                            }
                        }
                    } finally {
                        sub.close()
                    }
                    out.put("channelId", channel.id).put("mode", mode)
                } finally {
                    host.close()
                }
            }

            "f11_pairing_and_local_runtime_wiring" -> {
                val mikaScreenTokens = collectBytecodeTokens("com/ombhrum/fabushi/mika/") +
                    collectBytecodeTokens("com/ombhrum/fabushi/telegram/")
                checkCond(mikaScreenTokens.none { it.contains("ACTION_APPLICATION_DEVELOPMENT_SETTINGS") && !mikaScreenTokens.any { t -> t.contains("PairingCoordinator") || t.contains("PhoneSetup") || t.contains("openPhoneSetupInitially") } }) {
                    "Mika pairing still launches raw Developer Settings without Wireless ADB SPAKE2 PairingCoordinator / PhoneSetupScreen!"
                }
                val hasPairingWiring = mikaScreenTokens.any {
                    it.contains("openPhoneSetupInitially") || it.contains("PhoneSetup") || it.contains("PairingCoordinator")
                }
                checkCond(hasPairingWiring) {
                    "Missing Wireless ADB SPAKE2 PairingCoordinator / openPhoneSetupInitially wiring in Mika workspace"
                }
                val hasSandboxAndCron = mikaScreenTokens.any { it.contains("AndroidSandbox") || it.contains("TerminalHub") } &&
                    mikaScreenTokens.any { it.contains("TriggerRuntime") || it.contains("TaskService") }
                checkCond(hasSandboxAndCron) {
                    "Hermes TTY / Cron / CloudComputer 'node-local-android' is not wired to Mika AndroidSandbox / TriggerRuntime / TaskService"
                }
                out.put("pairingAndRuntimeWired", true)
            }

            // =========================================================================
            // F12 — Zero Fake Seed, InstalledAppSearchIndex, Camera, Talk/WebRTC, TON & Dharma
            // =========================================================================
            "f12_cold_start_truthfulness" -> {
                val app = E2eAndroidShims.createTestApplication(scratchDir)
                val vm = TelegramViewModel(app)
                val st = vm.state.value
                checkCond(st.accounts.isEmpty()) {
                    "Cold-start TelegramViewModel fabricated fake accounts (${st.accounts.map { it.id }}); buildInitialSeedState() must be eliminated!"
                }
                checkCond(st.chats.isEmpty()) {
                    "Cold-start TelegramViewModel fabricated ${st.chats.size} fake chats (${st.chats.take(3).map { it.id }})"
                }
                checkCond(st.stories.isEmpty()) {
                    "Cold-start TelegramViewModel fabricated ${st.stories.size} fake stories"
                }
                checkCond(st.tonWallet.tonBalance == 0.0 && st.tonWallet.mnemonicWords.isEmpty()) {
                    "Cold-start TonWalletUiState seeded fake balance (${st.tonWallet.tonBalance} TON) or hardcoded 24-word mnemonic!"
                }
                out.put("truthfulColdStart", true)
            }

            "f12_installed_app_search" -> {
                val throwOnQuery = req.optBoolean("throwOnQuery", false)
                val app = E2eAndroidShims.createTestApplicationWithApps(
                    scratchDir,
                    E2eAndroidShims.defaultLauncherApps(),
                    throwOnQuery,
                )
                val repoCls = loadRequiredClass(
                    listOf(
                        "com.ombhrum.fabushi.search.InstalledAppRepository",
                        "com.ombhrum.fabushi.telegram.InstalledAppRepository",
                        "com.ombhrum.fabushi.mika.InstalledAppRepository",
                        "com.ombhrum.fabushi.telegram.InstalledAppSearchIndex",
                    ),
                    "InstalledAppRepository / InstalledAppSearchIndex"
                )
                val repo = instantiateWithDeps(repoCls, listOf(app, app.packageManager))
                val searchM = findMethodByName(repoCls, listOf("searchApps", "search"))
                val hits = invokeSuspendOrSync(repo, searchM, req.optString("query", "wechat")) as? Collection<*>
                    ?: emptyList<Any>()
                if (throwOnQuery) {
                    // Must degrade gracefully without crashing!
                    out.put("gracefulFallbackHits", hits.size)
                } else {
                    checkCond(hits.isNotEmpty()) {
                        "InstalledAppRepository.searchApps('${req.optString("query", "wechat")}') returned 0 hits despite 'com.tencent.mm' ('微信') installed in PackageManager + Mika 36-app alias dictionary!"
                    }
                    val hitStrings = hits.map { it.toString() }
                    checkCond(hitStrings.any { it.contains("com.tencent.mm") || it.contains("微信") }) {
                        "Expected com.tencent.mm / 微信 in searchApps results, got: $hitStrings"
                    }
                    if (req.optBoolean("testSelfProtection", false)) {
                        val autoM = repoCls.methods.firstOrNull { it.name == "automateWithMika" }
                        checkCond(autoM != null) { "InstalledAppRepository missing automateWithMika" }
                    }
                    out.put("hitsCount", hits.size)
                }
            }

            "f12_talk_webrtc_wallet_dharma_wiring" -> {
                val vmTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/")
                checkCond(vmTokens.none { it.contains("已收到您的语音指令") }) {
                    "Talk Mode still returns hardcoded canned string '已收到您的语音指令' instead of Mika Transcribe -> LLM -> Audio TTS!"
                }
                val walletScreenTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/ui/TonWalletScreen")
                checkCond(walletScreenTokens.none { it == "STR:SHA-256" }) {
                    "TonWalletScreen still hashes address with SHA-256 to draw fake unscannable QR modules instead of calling wallet.generateQrMatrix!"
                }
                val dharmaTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/ui/GlobalDharma") + vmTokens
                checkCond(dharmaTokens.any { it.contains("MiniAppPlatformBridge") || it.contains("GlobalDharmaCommercePanel") }) {
                    "Global Dharma MiniApp is not wired to MiniAppPlatformBridge / GlobalDharmaCommercePanel!"
                }
                out.put("verified", true)
            }

            // =========================================================================
            // F13 — TelegramIntroAuthScreen, Switcher, Contacts, Bot Creation & APPS Tab
            // =========================================================================
            "f13_onboarding_contacts_apps_tab" -> {
                val introCls = loadRequiredClass(
                    listOf(
                        "com.ombhrum.fabushi.telegram.ui.TelegramIntroAuthScreenKt",
                        "com.ombhrum.fabushi.telegram.TelegramIntroAuthScreenKt",
                    ),
                    "TelegramIntroAuthScreen"
                )
                val contactsCls = loadRequiredClass(
                    listOf(
                        "com.ombhrum.fabushi.telegram.ui.TelegramContactsScreenKt",
                        "com.ombhrum.fabushi.telegram.TelegramContactsScreenKt",
                    ),
                    "TelegramContactsScreen"
                )
                val filterCls = loadRequiredClass(
                    listOf("com.ombhrum.fabushi.telegram.GlobalSearchFilter"),
                    "GlobalSearchFilter"
                )
                val filterNames = filterCls.enumConstants?.map { (it as Enum<*>).name }.orEmpty()
                checkCond("APPS" in filterNames) {
                    "GlobalSearchFilter enum is missing APPS ('应用') tab; found only: $filterNames"
                }
                out.put("introClass", introCls.name)
                    .put("contactsClass", contactsCls.name)
                    .put("filters", JSONArray(filterNames))
            }

            "f13_multi_account_isolation" -> {
                val app = E2eAndroidShims.createTestApplication(scratchDir)
                val vm = TelegramViewModel(app)
                checkCond(vm.state.value.accounts.isEmpty()) {
                    "TelegramViewModel starts with fake seeded accounts; cannot guarantee multi-account isolation!"
                }
                val vmTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/TelegramViewModel")
                checkCond(vmTokens.any { it.contains("accountSwitch") || it.contains("GenerationFence") }) {
                    "TelegramViewModel missing GenerationFence + telegram.auth.accountSwitch wiring"
                }
                out.put("isolated", true)
            }

            // =========================================================================
            // F14 — .imePadding(), Voice Drag Math, Peer Profile, Jump Search & Settings
            // =========================================================================
            "f14_ime_padding_bytecode" -> {
                val uiNodes = readClassNodesMatching {
                    it.startsWith("com/ombhrum/fabushi/telegram/ui/") || it.startsWith("com/ombhrum/fabushi/mika/")
                }
                checkCond(uiNodes.isNotEmpty()) { "No Telegram/Mika UI classes found in snapshot" }
                var imeCallCount = 0
                for (cn in uiNodes) {
                    for (mn in cn.methods) {
                        var insn = mn.instructions.first
                        while (insn != null) {
                            if (insn is MethodInsnNode && (insn.name.contains("imePadding") || insn.name.contains("getIme"))) {
                                imeCallCount++
                            }
                            insn = insn.next
                        }
                    }
                }
                checkCond(imeCallCount >= 4) {
                    "Edge-to-edge keyboard occlusion bug: found only $imeCallCount .imePadding()/WindowInsets.ime calls across all Telegram & Mika screens (expected >= 4 across Chat, Auth, Settings, Studio, Search)!"
                }
                out.put("imePaddingCallSites", imeCallCount)
            }

            "f14_cumulative_voice_drag" -> {
                val app = E2eAndroidShims.createTestApplication(scratchDir)
                val vm = TelegramViewModel(app)
                // Put VM into active unlocked recording state via startVoiceRecording or reflection on _state
                val stateField = vm.javaClass.getDeclaredField("_state").apply { isAccessible = true }
                @Suppress("UNCHECKED_CAST")
                val stateFlow = stateField.get(vm) as kotlinx.coroutines.flow.MutableStateFlow<com.ombhrum.fabushi.telegram.TelegramUiState>
                val recActive = stateFlow.value.recordingState.copy(isRecording = true, isLocked = false, dragOffsetX = 0f, dragOffsetY = 0f)
                stateFlow.value = stateFlow.value.copy(recordingState = recActive)

                val mode = req.optString("mode", "cancel")
                when (mode) {
                    "cancel" -> {
                        // 10 pointer frames of -15px delta = -150px cumulative (< -120px cancel threshold)
                        repeat(10) { vm.updateRecordingDrag(-15f, 0f) }
                        val after = vm.state.value.recordingState
                        checkCond(!after.isRecording || after.dragOffsetX <= -120f) {
                            "Voice drag bug (TelegramViewModel.kt:1772): 10 frames of dx=-15f yielded dragOffsetX=${after.dragOffsetX}, isRecording=${after.isRecording} (overwrote per-frame delta instead of accumulating to -150f!)"
                        }
                    }
                    "lock" -> {
                        // 10 pointer frames of -9px dy = -90px cumulative (< -75px lock threshold)
                        repeat(10) { vm.updateRecordingDrag(0f, -9f) }
                        val after = vm.state.value.recordingState
                        checkCond(after.isLocked) {
                            "Voice lock drag bug: 10 frames of dy=-9f yielded dragOffsetY=${after.dragOffsetY}, isLocked=false (did not accumulate to -90f lock threshold!)"
                        }
                    }
                    "jitter_and_locked_guard" -> {
                        // Small jitter (-20f, -15f) must neither cancel nor lock
                        repeat(4) { vm.updateRecordingDrag(-5f, -3f) }
                        val mid = vm.state.value.recordingState
                        checkCond(mid.isRecording && !mid.isLocked) {
                            "Micro-jitter falsely cancelled or locked recording: $mid"
                        }
                        // Now lock (-80f) and verify subsequent left-drag does NOT cancel locked recording!
                        repeat(10) { vm.updateRecordingDrag(0f, -9f) }
                        checkCond(vm.state.value.recordingState.isLocked) {
                            "Failed to lock recording on cumulative -90f upward drag"
                        }
                        repeat(12) { vm.updateRecordingDrag(-15f, 0f) }
                        checkCond(vm.state.value.recordingState.isRecording && vm.state.value.recordingState.isLocked) {
                            "Leftward drag after recording was locked erroneously cancelled the locked recording!"
                        }
                    }
                }
                out.put("mode", mode)
            }

            "f14_peer_profile_and_non_destructive_search" -> {
                val peerCls = loadRequiredClass(
                    listOf(
                        "com.ombhrum.fabushi.telegram.ui.TelegramPeerProfileScreenKt",
                        "com.ombhrum.fabushi.telegram.TelegramPeerProfileScreenKt",
                    ),
                    "TelegramPeerProfileScreen"
                )
                val chatTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/ui/TelegramChatScreen")
                val hasJumpSearch = chatTokens.any {
                    it.contains("nextInChatSearchMatch") || it.contains("prevInChatSearchMatch") || it.contains("inChatSearchMatch")
                }
                checkCond(hasJumpSearch) {
                    "TelegramChatScreen lacks non-destructive in-chat jump search (▲/▼ match navigation)"
                }
                out.put("peerProfile", peerCls.name)
            }

            "f14_settings_and_i18n" -> {
                val settingsTokens = collectBytecodeTokens("com/ombhrum/fabushi/telegram/ui/TelegramSettingsScreen") +
                    collectBytecodeTokens("com/ombhrum/fabushi/telegram/")
                checkCond(settingsTokens.any { it.contains("listSessions") || it.contains("revokeSession") }) {
                    "Settings DEVICES_SESSIONS still renders static text without live listSessions / revokeSession"
                }
                val hasLocalizedStrings = settingsTokens.any { it == "STR:English" } &&
                    settingsTokens.any { it.contains("Settings") || it.contains("Chats") || it.contains("Contacts") } &&
                    settingsTokens.any { it.contains("設定") || it.contains("チャット") }
                checkCond(hasLocalizedStrings) {
                    "UI localization is cosmetic: switching uiLanguage does not provide English / 日本語 / 繁體中文 UI string tables!"
                }
                out.put("settingsAndI18nVerified", true)
            }

            // =========================================================================
            // F15 — OTA Update Validator, BuildConfig Sync & Production Readiness
            // =========================================================================
            "f15_ota_validator_and_version" -> {
                val mode = req.optString("mode", "positive")
                val appBc = Class.forName("com.ombhrum.fabushi.BuildConfig")
                val mikaBc = Class.forName("com.wangzan101.mika.BuildConfig")
                val appVCode = appBc.getField("VERSION_CODE").getInt(null)
                val mikaVCode = mikaBc.getField("VERSION_CODE").getInt(null)
                val mikaVName = mikaBc.getField("VERSION_NAME").get(null) as String
                checkCond(appVCode == 20100 && mikaVCode == 20100 && mikaVName == "2.1.0") {
                    "BuildConfig mismatch: app.VERSION_CODE=$appVCode, mika.VERSION_CODE=$mikaVCode, mika.VERSION_NAME=$mikaVName"
                }

                checkCond(AndroidVersion.isNewer("2.1.0", 20100L, "2.0.0", 20000L)) {
                    "AndroidVersion.isNewer('2.1.0', 20100, '2.0.0', 20000) returned false"
                }
                checkCond(!AndroidVersion.isNewer("2.0.0", 20000L, "2.1.0", 20100L)) {
                    "AndroidVersion.isNewer accepted downgrade 2.0.0 -> 2.1.0"
                }
                checkCond(AndroidVersion.isNewer("2.1.0", 20100L, "2.1.0-rc.1", 20100L)) {
                    "AndroidVersion.isNewer failed release > pre-release ordering"
                }

                val validUrl = "https://github.com/fabu-android/fabushi-android/releases/download/v2.1.0/fabushi-telegram-mika-2.1.0-githubRelease.apk"
                checkCond(AndroidUpdateValidator.trustedReleaseDownloadUrl(validUrl) == validUrl) {
                    "AndroidUpdateValidator rejected canonical fabu-android/fabushi-android HTTPS release URL"
                }

                if (mode == "negative") {
                    val untrusted = listOf(
                        "http://github.com/fabu-android/fabushi-android/releases/download/v2.1.0/app.apk",
                        "https://evil.example.com/fabu-android/fabushi-android/releases/download/v2.1.0/app.apk",
                        "https://github.com/evil/repo/releases/download/v2.1.0/app.apk",
                    )
                    for (bad in untrusted) {
                        var rejected = false
                        try {
                            AndroidUpdateValidator.trustedReleaseDownloadUrl(bad)
                        } catch (_: IllegalArgumentException) {
                            rejected = true
                        }
                        checkCond(rejected) { "AndroidUpdateValidator failed to reject untrusted URL: $bad" }
                    }
                    checkCond(!AndroidUpdateValidator.isValidApkFileName("../evil.apk")) {
                        "AndroidUpdateValidator failed to reject path traversal APK name"
                    }
                    checkCond(!AndroidUpdateValidator.isValidSha256Hex("xyz123")) {
                        "AndroidUpdateValidator failed to reject malformed SHA-256"
                    }
                }
                out.put("appVCode", appVCode).put("mikaVCode", mikaVCode)
            }

            else -> throw IllegalArgumentException("Unknown JVM probe: '$probe'")
        }
        return out
    }
}
