package com.ombhrum.fabushi.e2e;

import java.io.File;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.Callable;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

/**
 * External OpenJDK 17 JNI probe exercising com.ombhrum.fabushi.core.MahayanaHost
 * against compiled libmahayana_app_host.so (Modified-UTF-8 astral emoji surrogates,
 * multi-consumer ref-counting, feature.receive fan-out, and thread concurrency).
 */
public final class JniBridgeProbe {

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("Usage: JniBridgeProbe <mode> <filesDir>");
            System.exit(3);
        }
        String mode = args[0];
        File filesDir = new File(args[1]);
        filesDir.mkdirs();

        Class<?> ctxShimCls = Class.forName("com.ombhrum.fabushi.e2e.E2eAndroidShims");
        Method createApp = ctxShimCls.getMethod("createTestApplication", File.class);
        Object appCtx = createApp.invoke(null, filesDir);

        Class<?> hostCls = Class.forName("com.ombhrum.fabushi.core.MahayanaHost");
        Class<?> androidCtxCls = Class.forName("android.content.Context");
        Class<?> jsonObjCls = Class.forName("org.json.JSONObject");
        Constructor<?> hostCtor = hostCls.getConstructor(androidCtxCls, boolean.class);
        Method requestMethod = hostCls.getMethod("request", String.class, jsonObjCls);
        Method publishMethod = hostCls.getMethod("publishFeatureEvent", jsonObjCls);
        Method closeMethod = hostCls.getMethod("close");

        if ("surrogate_roundtrip".equals(mode)) {
            Object host = hostCtor.newInstance(appCtx, Boolean.TRUE);
            try {
                StringBuilder big = new StringBuilder(140_000);
                String unit = "全球法布施·般若波罗蜜🪷🙏☸️🔐🦋👨‍👩‍👧‍👦\"\\\n\t";
                for (int i = 0; i < 3200; i++) {
                    big.append(unit).append('#').append(i);
                }
                String expected = big.toString();

                Object setParams = jsonObjCls.getConstructor().newInstance();
                Method put = jsonObjCls.getMethod("put", String.class, Object.class);
                put.invoke(setParams, "key", "jni_astral_key_🪷");
                put.invoke(setParams, "value", expected);
                requestMethod.invoke(host, "feature.persistence.set", setParams);

                Object getParams = jsonObjCls.getConstructor().newInstance();
                put.invoke(getParams, "key", "jni_astral_key_🪷");
                Object resultObj = requestMethod.invoke(host, "feature.persistence.get", getParams);
                Method optStr = jsonObjCls.getMethod("optString", String.class, String.class);
                String actual = (String) optStr.invoke(resultObj, "value", "");

                if (!expected.equals(actual)) {
                    System.err.println(
                        "JNI surrogate mismatch: expected len=" + expected.length() + " got len=" + actual.length()
                    );
                    System.exit(2);
                }
                System.out.println("PASS:surrogate_roundtrip:" + actual.getBytes(StandardCharsets.UTF_8).length);
            } finally {
                closeMethod.invoke(host);
            }
            return;
        }

        if ("shared_refcount_fanout".equals(mode)) {
            Object hostA = hostCtor.newInstance(appCtx, Boolean.FALSE);
            Object hostB = hostCtor.newInstance(appCtx, Boolean.FALSE);
            Method put = jsonObjCls.getMethod("put", String.class, Object.class);
            Method optStr = jsonObjCls.getMethod("optString", String.class, String.class);
            try {
                Object ev = jsonObjCls.getConstructor().newInstance();
                put.invoke(ev, "type", "e2e.fanout.event");
                put.invoke(ev, "marker", "shared_truth_108");
                publishMethod.invoke(hostA, ev);

                Object empty = jsonObjCls.getConstructor().newInstance();
                Object rxA = requestMethod.invoke(hostA, "feature.receive", empty);
                Object rxB = requestMethod.invoke(hostB, "feature.receive", empty);
                String typeA = (String) optStr.invoke(rxA, "type", "");
                String typeB = (String) optStr.invoke(rxB, "type", "");
                if (!"e2e.fanout.event".equals(typeA) || !"e2e.fanout.event".equals(typeB)) {
                    System.err.println("Fan-out failed: rxA=" + rxA + " rxB=" + rxB);
                    System.exit(2);
                }

                // Close A while B remains open; B must continue dispatching cleanly!
                closeMethod.invoke(hostA);
                Object info = requestMethod.invoke(hostB, "feature.info", empty);
                String platform = (String) optStr.invoke(info, "platform", "");
                if (!"android".equals(platform)) {
                    System.err.println("HostB broke after closing HostA: " + info);
                    System.exit(2);
                }
                System.out.println("PASS:shared_refcount_fanout");
            } finally {
                try { closeMethod.invoke(hostA); } catch (Throwable ignored) {}
                try { closeMethod.invoke(hostB); } catch (Throwable ignored) {}
            }
            return;
        }

        if ("concurrent_jni_stress".equals(mode)) {
            Object host = hostCtor.newInstance(appCtx, Boolean.FALSE);
            ExecutorService pool = Executors.newFixedThreadPool(8);
            Method put = jsonObjCls.getMethod("put", String.class, Object.class);
            Method optStr = jsonObjCls.getMethod("optString", String.class, String.class);
            try {
                List<Callable<Boolean>> tasks = new ArrayList<>();
                for (int t = 0; t < 8; t++) {
                    final int tid = t;
                    tasks.add(() -> {
                        for (int i = 0; i < 25; i++) {
                            String k = "k_" + tid + "_" + i;
                            String v = "val_☸️_" + tid + "_" + i;
                            Object pSet = jsonObjCls.getConstructor().newInstance();
                            put.invoke(pSet, "key", k);
                            put.invoke(pSet, "value", v);
                            requestMethod.invoke(host, "feature.persistence.set", pSet);

                            Object pGet = jsonObjCls.getConstructor().newInstance();
                            put.invoke(pGet, "key", k);
                            Object res = requestMethod.invoke(host, "feature.persistence.get", pGet);
                            if (!v.equals(optStr.invoke(res, "value", ""))) {
                                return Boolean.FALSE;
                            }
                        }
                        return Boolean.TRUE;
                    });
                }
                for (Future<Boolean> f : pool.invokeAll(tasks)) {
                    if (!Boolean.TRUE.equals(f.get())) {
                        System.err.println("Concurrent JNI persistence mismatch");
                        System.exit(2);
                    }
                }
                System.out.println("PASS:concurrent_jni_stress:200");
            } finally {
                pool.shutdownNow();
                closeMethod.invoke(host);
            }
            return;
        }

        System.err.println("Unknown mode: " + mode);
        System.exit(3);
    }
}
