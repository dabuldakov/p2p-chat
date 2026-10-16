# Правила R8/ProGuard для release-сборки p2pchat.

# --- UniFFI (сгенерированные биндинги) + нативный загрузчик ---
-keep class uniffi.** { *; }
-keepclasseswithmembernames class * { native <methods>; }

# --- JNA (через него UniFFI грузит .so) ---
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure { *; }
-keep class com.sun.jna.Library
-keep class com.sun.jna.Native
-keep class com.sun.jna.NativeLibrary
-keep class com.sun.jna.Pointer
-keep class com.sun.jna.Callback
-dontwarn com.sun.jna.**
-dontwarn java.awt.**

# --- WebRTC ---
-keep class org.webrtc.** { *; }
-dontwarn org.webrtc.**

# --- protobuf-lite (generated) ---
-keep class com.google.protobuf.** { *; }
-keep class com.p2pchat.proto.** { *; }
-keepclassmembers class com.p2pchat.proto.** { *; }

# --- kotlinx.coroutines ---
-dontwarn kotlinx.coroutines.**
-keepclassmembers class kotlinx.coroutines.** { volatile <fields>; }

# --- Kotlin/Reflection ---
-keepattributes *Annotation*, InnerClasses, Signature, EnclosingMethod
-dontwarn kotlin.**
-dontwarn kotlinx.**

# --- Наше приложение ---
-keep class com.p2pchat.app.** { *; }

# --- kotlinx.serialization ---
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault
-keepclassmembers class kotlinx.serialization.json.** { *; }
-keep,includedescriptorclasses class com.p2pchat.app.gif.**$$serializer { *; }
-keepclassmembers class com.p2pchat.app.gif.**$Companion { *; }
-keepclasseswithmembers class com.p2pchat.app.gif.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# --- Coil / Media3 / OkHttp (поставляют consumer-правила) ---
-dontwarn coil.**
-dontwarn androidx.media3.**
-dontwarn okhttp3.**
-dontwarn okio.**

