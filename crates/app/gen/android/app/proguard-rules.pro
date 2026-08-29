# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# Whatever the Rust side calls over JNI has no caller R8 can see, so it is dropped from the release
# build, which is the only minified one: the call then fails with `NoSuchMethodError` while the
# debug build works. Everything reached by name from native code belongs here.
-keepclassmembers class org.laplace.LappActivity {
    public static boolean moveToFront(java.lang.String);
}
