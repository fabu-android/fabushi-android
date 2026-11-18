import com.google.zxing.BarcodeFormat;
import com.google.zxing.BinaryBitmap;
import com.google.zxing.DecodeHintType;
import com.google.zxing.EncodeHintType;
import com.google.zxing.RGBLuminanceSource;
import com.google.zxing.Result;
import com.google.zxing.common.BitMatrix;
import com.google.zxing.common.HybridBinarizer;
import com.google.zxing.qrcode.QRCodeReader;
import com.google.zxing.qrcode.QRCodeWriter;
import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.Map;

/**
 * Headless ISO/IEC 18004 optical QR verifier & encoder backed by com.google.zxing:core:3.5.3.
 * Renders boolean module matrices with a 4-module white quiet zone at 8x8 pixels/module (>= 232px)
 * so HybridBinarizer (40x40 minimum block threshold) and finder-pattern detection work identically
 * to a real phone camera scan.
 */
public final class ZxingQrProbe {
    private static final int QUIET_ZONE_MODULES = 4;
    private static final int SCALE_PX = 8;

    public static void main(String[] args) throws Exception {
        String mode = args.length > 0 ? args[0] : "decode";
        String input = readStdinUtf8().trim();

        if ("encode".equals(mode)) {
            Map<EncodeHintType, Object> hints = new EnumMap<>(EncodeHintType.class);
            hints.put(EncodeHintType.CHARACTER_SET, "UTF-8");
            hints.put(EncodeHintType.MARGIN, 0);
            BitMatrix matrix = new QRCodeWriter().encode(input, BarcodeFormat.QR_CODE, 0, 0, hints);
            int w = matrix.getWidth();
            StringBuilder sb = new StringBuilder(w * w * 6 + 32);
            sb.append("{\"size\":").append(w).append(",\"modules\":[");
            boolean first = true;
            for (int y = 0; y < w; y++) {
                for (int x = 0; x < w; x++) {
                    if (!first) sb.append(',');
                    first = false;
                    sb.append(matrix.get(x, y) ? "true" : "false");
                }
            }
            sb.append("]}");
            System.out.println(sb.toString());
            return;
        }

        // Decode {"size": N, "modules": [bool, ...]}
        int size = parseSize(input);
        List<Boolean> modules = parseModules(input);
        if (size <= 0 || modules.size() != size * size) {
            System.err.println("INVALID_MATRIX: size=" + size + " modules=" + modules.size());
            System.exit(3);
        }

        int totalModules = size + 2 * QUIET_ZONE_MODULES;
        int imgDim = totalModules * SCALE_PX;
        int[] pixels = new int[imgDim * imgDim];
        for (int i = 0; i < pixels.length; i++) {
            pixels[i] = 0xFFFFFFFF;
        }
        for (int r = 0; r < size; r++) {
            for (int c = 0; c < size; c++) {
                if (Boolean.TRUE.equals(modules.get(r * size + c))) {
                    int y0 = (r + QUIET_ZONE_MODULES) * SCALE_PX;
                    int x0 = (c + QUIET_ZONE_MODULES) * SCALE_PX;
                    for (int dy = 0; dy < SCALE_PX; dy++) {
                        int rowBase = (y0 + dy) * imgDim;
                        for (int dx = 0; dx < SCALE_PX; dx++) {
                            pixels[rowBase + x0 + dx] = 0xFF000000;
                        }
                    }
                }
            }
        }

        try {
            RGBLuminanceSource source = new RGBLuminanceSource(imgDim, imgDim, pixels);
            // 1. Optical finder-pattern decode (no PURE_BARCODE shortcut!)
            Map<DecodeHintType, Object> cameraHints = new EnumMap<>(DecodeHintType.class);
            cameraHints.put(DecodeHintType.CHARACTER_SET, "UTF-8");
            cameraHints.put(DecodeHintType.TRY_HARDER, Boolean.TRUE);
            Result cameraResult = new QRCodeReader().decode(new BinaryBitmap(new HybridBinarizer(source)), cameraHints);

            // 2. Pure grid cross-check
            Map<DecodeHintType, Object> pureHints = new EnumMap<>(DecodeHintType.class);
            pureHints.put(DecodeHintType.CHARACTER_SET, "UTF-8");
            pureHints.put(DecodeHintType.PURE_BARCODE, Boolean.TRUE);
            Result pureResult = new QRCodeReader().decode(new BinaryBitmap(new HybridBinarizer(source)), pureHints);

            if (!cameraResult.getText().equals(pureResult.getText())) {
                System.err.println("MISMATCH: camera=" + cameraResult.getText() + " pure=" + pureResult.getText());
                System.exit(2);
            }
            System.out.print(cameraResult.getText());
        } catch (Exception ex) {
            System.err.println(ex.getClass().getSimpleName() + ": " + ex.getMessage());
            System.exit(2);
        }
    }

    private static String readStdinUtf8() throws Exception {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[4096];
        int n;
        while ((n = System.in.read(buf)) != -1) {
            out.write(buf, 0, n);
        }
        return out.toString(StandardCharsets.UTF_8);
    }

    private static int parseSize(String json) {
        int idx = json.indexOf("\"size\"");
        if (idx < 0) return -1;
        int colon = json.indexOf(':', idx);
        if (colon < 0) return -1;
        int i = colon + 1;
        while (i < json.length() && Character.isWhitespace(json.charAt(i))) i++;
        int start = i;
        while (i < json.length() && (Character.isDigit(json.charAt(i)) || json.charAt(i) == '-')) i++;
        return start < i ? Integer.parseInt(json.substring(start, i)) : -1;
    }

    private static List<Boolean> parseModules(String json) {
        List<Boolean> list = new ArrayList<>();
        int idx = json.indexOf("\"modules\"");
        if (idx < 0) return list;
        int open = json.indexOf('[', idx);
        int close = json.indexOf(']', open);
        if (open < 0 || close < 0) return list;
        String body = json.substring(open + 1, close);
        int pos = 0;
        while (pos < body.length()) {
            char ch = body.charAt(pos);
            if (ch == 't' && body.startsWith("true", pos)) {
                list.add(Boolean.TRUE);
                pos += 4;
            } else if (ch == 'f' && body.startsWith("false", pos)) {
                list.add(Boolean.FALSE);
                pos += 5;
            } else if (ch == '1') {
                list.add(Boolean.TRUE);
                pos += 1;
            } else if (ch == '0') {
                list.add(Boolean.FALSE);
                pos += 1;
            } else {
                pos += 1;
            }
        }
        return list;
    }
}
