# ============================================================================
# Fabushi Android (GPUI-Telegram + Mahayana Rust JNI + Mika Phone Agent)
# Production R8 / ProGuard Rules
# ============================================================================

-keepattributes SourceFile,LineNumberTable,Signature,InnerClasses,EnclosingMethod,*Annotation*,JavascriptInterface

# ----------------------------------------------------------------------------
# 1. Rust JNI Host Bridge (libmahayana_app_host.so -> MahayanaHost)
# ----------------------------------------------------------------------------
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}

-keep class com.ombhrum.fabushi.core.MahayanaHost {
    public <init>(...);
    <methods>;
}

# ----------------------------------------------------------------------------
# 2. WebView @JavascriptInterface & MiniApp WebMCP Bridge
# ----------------------------------------------------------------------------
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

-keep class com.ombhrum.fabushi.MiniAppNativeWebMcpBridge {
    *;
}

# ----------------------------------------------------------------------------
# 3. Mika On-Device Phone Control (:mika-core, :mika-agent, app_process Helper & AIDL)
# ----------------------------------------------------------------------------
-keep class * extends android.os.Binder { *; }
-keep interface * extends android.os.IInterface { *; }

# Keep app_process privileged helper main(String[]) entrypoints across Mika packages
-keepclasseswithmembers public class * {
    public static void main(java.lang.String[]);
}

-keep class **.*HelperMain { *; }
-keep class **.*MikaShell* { *; }
-keep class com.wangzan101.mika.cap.phone.shell.** { *; }
-keep class com.ombhrum.fabushi.mika.** { *; }
-keep class com.mika.** { *; }
-keep class ai.mika.** { *; }

# ----------------------------------------------------------------------------
# 4. Telegram Android UI & ViewModels
# ----------------------------------------------------------------------------
-keep class * extends androidx.lifecycle.ViewModel {
    public <init>();
    public <init>(android.app.Application);
}

-keepclassmembers enum com.ombhrum.fabushi.** {
    public static **[] values();
    public static ** valueOf(java.lang.String);
}

# ----------------------------------------------------------------------------
# 5. kotlinx.serialization Reflection-Free Serializer Keep Rules
# ----------------------------------------------------------------------------
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault

-if @kotlinx.serialization.Serializable class **
-keepclassmembers class <1> {
    static <1>$Companion Companion;
}

-if @kotlinx.serialization.Serializable class ** {
    static **$* *;
}
-keepclassmembers class <2>$<3> {
    kotlinx.serialization.KSerializer serializer(...);
}

-if @kotlinx.serialization.Serializable class **
-keep,includedescriptorclasses class <1>$$serializer { *; }

-keepclassmembers class ** {
    @kotlinx.serialization.SerialName <fields>;
}

# ----------------------------------------------------------------------------
# 6. Protobuf / OpenTelemetry OTLP & Google MLKit OCR
# ----------------------------------------------------------------------------
-keep class * extends com.google.protobuf.GeneratedMessageLite { *; }
-keep class io.opentelemetry.** { *; }
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.odml.** { *; }

# ----------------------------------------------------------------------------
# 7. OkHttp / Commons-Compress Optional Codecs & Safe Platform Warnings
# ----------------------------------------------------------------------------
-dontwarn com.github.luben.zstd.**
-dontwarn org.brotli.**
-dontwarn okhttp3.internal.platform.**
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**
-dontwarn javax.annotation.**
-dontwarn sun.misc.Unsafe
-dontwarn com.google.errorprone.annotations.**
