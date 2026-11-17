# ==============================================================================
# Mika On-Device Phone Control Library (:mika-agent & :mika-core) Consumer Rules
# ==============================================================================

# 1. Standalone UID 2000 Binder Helper launched via `app_process /system/bin`
-keep class com.wangzan101.mika.shell.HelperMain {
    public static void main(java.lang.String[]);
    *;
}
-keep class com.wangzan101.mika.shell.** { *; }
-keep class com.wangzan101.mika.cap.phone.shell.** { *; }
-keep interface com.wangzan101.mika.shell.IMikaShell { *; }
-keep class com.wangzan101.mika.shell.IMikaShell$* { *; }
-keep class * extends com.wangzan101.mika.shell.IMikaShell$Stub { *; }

# 2. Android Manifest & Application entrypoints
-keep class com.wangzan101.mika.MikaApp { *; }
-keep class * extends com.wangzan101.mika.MikaApp { *; }

# 3. kotlinx.serialization (@Serializable, generated $$serializer, Companion)
-keepattributes *Annotation*, InnerClasses, EnclosingMethod, Signature
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
-keepclassmembers class kotlinx.serialization.json.** {
    *** Companion;
}
-keepclasseswithmembers class kotlinx.serialization.json.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# 4. OpenTelemetry & Protocol Buffers Lite (generated message fields/methods accessed via reflection)
-keep class io.opentelemetry.proto.** { *; }
-keep class com.google.protobuf.** { *; }
-dontwarn com.google.protobuf.**

# 5. Google ML Kit Text Recognition (Latin + Chinese bundled models) & Barhopper
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.libraries.barhopper.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_text_common.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_text_bundled_common.** { *; }
-keep @com.google.android.gms.common.annotation.KeepForSdk class * { *; }
-keep @com.google.android.gms.common.annotation.KeepName class * { *; }
-dontwarn com.google.mlkit.**
-dontwarn com.google.android.libraries.barhopper.**

# 6. WebView @JavascriptInterface bridges (CDP / Terminal / SelfCheck)
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}
-keep @android.webkit.JavascriptInterface class * { *; }

# 7. Mika Resource IDs (referenced dynamically or in diagnostics/notifications)
-keepclassmembers class com.wangzan101.mika.R$* {
    public static <fields>;
}

# 8. OkHttp 5 / Okio / Conscrypt optional platform warnings
-dontwarn okhttp3.internal.platform.**
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**
