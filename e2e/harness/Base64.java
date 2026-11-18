package android.util;

import java.nio.charset.StandardCharsets;

/**
 * Host-JVM functional implementation of android.util.Base64 backed by java.util.Base64
 * so Android classes encoding/decoding ciphertexts or blobs work identically on OpenJDK 17.
 */
public final class Base64 {
    public static final int DEFAULT = 0;
    public static final int NO_PADDING = 1;
    public static final int NO_WRAP = 2;
    public static final int CRLF = 4;
    public static final int URL_SAFE = 8;
    public static final int NO_CLOSE = 16;

    private Base64() {}

    public static byte[] decode(String str, int flags) {
        if (str == null) throw new IllegalArgumentException("null base64 string");
        String compact = str.replace("\r", "").replace("\n", "").trim();
        if ((flags & URL_SAFE) != 0 || compact.indexOf('-') >= 0 || compact.indexOf('_') >= 0) {
            return java.util.Base64.getUrlDecoder().decode(compact);
        }
        return java.util.Base64.getDecoder().decode(compact);
    }

    public static byte[] decode(byte[] input, int flags) {
        return decode(new String(input, StandardCharsets.ISO_8859_1), flags);
    }

    public static byte[] decode(byte[] input, int offset, int len, int flags) {
        return decode(new String(input, offset, len, StandardCharsets.ISO_8859_1), flags);
    }

    public static String encodeToString(byte[] input, int flags) {
        if (input == null) return null;
        java.util.Base64.Encoder enc = (flags & URL_SAFE) != 0
            ? java.util.Base64.getUrlEncoder()
            : java.util.Base64.getEncoder();
        if ((flags & NO_PADDING) != 0) {
            enc = enc.withoutPadding();
        }
        String out = enc.encodeToString(input);
        if ((flags & NO_WRAP) == 0 && out.length() > 76) {
            return out + "\n";
        }
        return out;
    }

    public static String encodeToString(byte[] input, int offset, int len, int flags) {
        byte[] slice = new byte[len];
        System.arraycopy(input, offset, slice, 0, len);
        return encodeToString(slice, flags);
    }

    public static byte[] encode(byte[] input, int flags) {
        return encodeToString(input, flags).getBytes(StandardCharsets.ISO_8859_1);
    }

    public static byte[] encode(byte[] input, int offset, int len, int flags) {
        return encodeToString(input, offset, len, flags).getBytes(StandardCharsets.ISO_8859_1);
    }
}
