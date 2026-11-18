package com.ombhrum.fabushi.e2e;

import android.app.Application;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import java.io.File;
import java.io.InputStream;
import java.io.OutputStream;
import java.security.Key;
import java.security.KeyStore;
import java.security.KeyStoreSpi;
import java.security.Provider;
import java.security.SecureRandom;
import java.security.Security;
import java.security.cert.Certificate;
import java.security.spec.AlgorithmParameterSpec;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Date;
import java.util.Enumeration;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import javax.crypto.KeyGenerator;
import javax.crypto.KeyGeneratorSpi;
import javax.crypto.SecretKey;

/**
 * Lightweight host-JVM Android runtime shims for opaque-box E2E probes:
 * 1. Registers a real JCE AES-256-GCM "AndroidKeyStore" provider so KeystoreKeyCipher
 *    and AiCredentialVault encrypt/decrypt genuinely without hardware TEE.
 * 2. Provides TestApplication with real file-backed filesDir/cacheDir, working in-memory
 *    SharedPreferences, and configurable PackageManager launcher activities.
 */
public final class E2eAndroidShims {

    private static final Map<String, SecretKey> KEYSTORE_KEYS = new ConcurrentHashMap<>();
    private static volatile boolean keystoreInstalled = false;

    public static synchronized void ensureSoftwareAndroidKeyStore() {
        if (keystoreInstalled) return;
        if (Security.getProvider("AndroidKeyStore") == null) {
            Security.addProvider(new SoftwareAndroidKeyStoreProvider());
        }
        keystoreInstalled = true;
    }

    public static void clearSoftwareAndroidKeyStore() {
        KEYSTORE_KEYS.clear();
    }

    public static Application createTestApplication(File baseDir) {
        ensureSoftwareAndroidKeyStore();
        return new TestApplication(baseDir, defaultLauncherApps(), false);
    }

    public static Application createTestApplicationWithApps(
        File baseDir,
        List<String[]> pkgAndLabels,
        boolean throwOnQuery
    ) {
        ensureSoftwareAndroidKeyStore();
        return new TestApplication(baseDir, pkgAndLabels, throwOnQuery);
    }

    public static List<String[]> defaultLauncherApps() {
        List<String[]> list = new ArrayList<>();
        list.add(new String[] {"com.tencent.mm", "微信", ".ui.LauncherUI"});
        list.add(new String[] {"com.sankuai.meituan", "美团", ".activity.MainActivity"});
        list.add(new String[] {"com.eg.android.AlipayGphone", "支付宝", ".AlipayLogin"});
        list.add(new String[] {"com.ombhrum.fabushi", "Fabushi", ".MainActivity"});
        return list;
    }

    public static final class SoftwareAndroidKeyStoreProvider extends Provider {
        @SuppressWarnings("deprecation")
        public SoftwareAndroidKeyStoreProvider() {
            super("AndroidKeyStore", 1.0, "E2E JCE AES-256-GCM AndroidKeyStore Shim");
            put("KeyStore.AndroidKeyStore", MemKeyStoreSpi.class.getName());
            put("KeyGenerator.AES", MemAesKeyGeneratorSpi.class.getName());
        }
    }

    public static final class MemKeyStoreSpi extends KeyStoreSpi {
        @Override
        public Key engineGetKey(String alias, char[] password) {
            return KEYSTORE_KEYS.get(alias);
        }

        @Override
        public Certificate[] engineGetCertificateChain(String alias) {
            return null;
        }

        @Override
        public Certificate engineGetCertificate(String alias) {
            return null;
        }

        @Override
        public Date engineGetCreationDate(String alias) {
            return KEYSTORE_KEYS.containsKey(alias) ? new Date() : null;
        }

        @Override
        public void engineSetKeyEntry(String alias, Key key, char[] password, Certificate[] chain) {
            if (key instanceof SecretKey) {
                KEYSTORE_KEYS.put(alias, (SecretKey) key);
            }
        }

        @Override
        public void engineSetKeyEntry(String alias, byte[] key, Certificate[] chain) {}

        @Override
        public void engineSetCertificateEntry(String alias, Certificate cert) {}

        @Override
        public void engineDeleteEntry(String alias) {
            KEYSTORE_KEYS.remove(alias);
        }

        @Override
        public Enumeration<String> engineAliases() {
            return Collections.enumeration(KEYSTORE_KEYS.keySet());
        }

        @Override
        public boolean engineContainsAlias(String alias) {
            return KEYSTORE_KEYS.containsKey(alias);
        }

        @Override
        public int engineSize() {
            return KEYSTORE_KEYS.size();
        }

        @Override
        public boolean engineIsKeyEntry(String alias) {
            return KEYSTORE_KEYS.containsKey(alias);
        }

        @Override
        public boolean engineIsCertificateEntry(String alias) {
            return false;
        }

        @Override
        public String engineGetCertificateAlias(Certificate cert) {
            return null;
        }

        @Override
        public void engineStore(OutputStream stream, char[] password) {}

        @Override
        public void engineLoad(InputStream stream, char[] password) {}
    }

    public static final class MemAesKeyGeneratorSpi extends KeyGeneratorSpi {
        private String alias = "default-e2e-alias";
        private int keyBits = 256;

        @Override
        protected void engineInit(SecureRandom random) {}

        @Override
        protected void engineInit(AlgorithmParameterSpec params, SecureRandom random) {
            if (params != null) {
                try {
                    Object got = params.getClass().getMethod("getKeystoreAlias").invoke(params);
                    if (got instanceof String && !((String) got).isEmpty()) {
                        alias = (String) got;
                    }
                } catch (Throwable ignored) {}
            }
        }

        @Override
        protected void engineInit(int keysize, SecureRandom random) {
            this.keyBits = keysize > 0 ? keysize : 256;
        }

        @Override
        protected SecretKey engineGenerateKey() {
            try {
                KeyGenerator jce = KeyGenerator.getInstance("AES");
                jce.init(keyBits);
                SecretKey sk = jce.generateKey();
                KEYSTORE_KEYS.put(alias, sk);
                return sk;
            } catch (Exception ex) {
                throw new RuntimeException(ex);
            }
        }
    }

    public static final class TestApplication extends Application {
        private final File baseDir;
        private final File filesDir;
        private final File cacheDir;
        private final Map<String, InMemoryPrefs> prefsMap = new ConcurrentHashMap<>();
        private final PackageManager pm;

        public TestApplication(File baseDir, List<String[]> apps, boolean throwOnQuery) {
            this.baseDir = baseDir;
            this.filesDir = new File(baseDir, "files");
            this.cacheDir = new File(baseDir, "cache");
            this.filesDir.mkdirs();
            this.cacheDir.mkdirs();
            this.pm = createPackageManagerProxy(apps, throwOnQuery);
        }

        @Override
        public Context getApplicationContext() {
            return this;
        }

        @Override
        public File getFilesDir() {
            filesDir.mkdirs();
            return filesDir;
        }

        @Override
        public File getCacheDir() {
            cacheDir.mkdirs();
            return cacheDir;
        }

        @Override
        public File getDir(String name, int mode) {
            File d = new File(baseDir, "app_" + name);
            d.mkdirs();
            return d;
        }

        @Override
        public String getPackageName() {
            return "com.ombhrum.fabushi";
        }

        @Override
        public SharedPreferences getSharedPreferences(String name, int mode) {
            return prefsMap.computeIfAbsent(name == null ? "default" : name, k -> new InMemoryPrefs());
        }

        @Override
        public PackageManager getPackageManager() {
            return pm;
        }

        public Map<String, Object> dumpAllPreferences() {
            Map<String, Object> all = new HashMap<>();
            for (Map.Entry<String, InMemoryPrefs> e : prefsMap.entrySet()) {
                all.put(e.getKey(), e.getValue().getAll());
            }
            return all;
        }
    }

    private static PackageManager createPackageManagerProxy(List<String[]> apps, boolean throwOnQuery) {
        return new MockLauncherPackageManager(apps, throwOnQuery);
    }

    public static class MockLauncherPackageManager extends PackageManager {
        private final List<String[]> apps;
        private final boolean throwOnQuery;

        public MockLauncherPackageManager(List<String[]> apps, boolean throwOnQuery) {
            this.apps = apps != null ? apps : Collections.emptyList();
            this.throwOnQuery = throwOnQuery;
        }

        @Override
        @SuppressWarnings("deprecation")
        public List<ResolveInfo> queryIntentActivities(Intent intent, int flags) {
            if (throwOnQuery) {
                throw new SecurityException("Simulated PackageManager query SecurityException");
            }
            List<ResolveInfo> out = new ArrayList<>();
            for (String[] row : apps) {
                ResolveInfo ri = new ResolveInfo();
                ActivityInfo ai = new ActivityInfo();
                ApplicationInfo appInfo = new ApplicationInfo();
                appInfo.packageName = row[0];
                appInfo.nonLocalizedLabel = row[1];
                ai.packageName = row[0];
                ai.name = row.length > 2 ? row[0] + row[2] : row[0] + ".MainActivity";
                ai.applicationInfo = appInfo;
                ai.nonLocalizedLabel = row[1];
                ri.activityInfo = ai;
                ri.nonLocalizedLabel = row[1];
                out.add(ri);
            }
            return out;
        }

        @Override
        public CharSequence getApplicationLabel(ApplicationInfo info) {
            if (info != null && info.nonLocalizedLabel != null) {
                return info.nonLocalizedLabel;
            }
            if (info != null && info.packageName != null) {
                for (String[] row : apps) {
                    if (row[0].equals(info.packageName)) return row[1];
                }
                return info.packageName;
            }
            return "";
        }

        @Override
        public Intent getLaunchIntentForPackage(String packageName) {
            for (String[] row : apps) {
                if (row[0].equals(packageName)) {
                    Intent i = new Intent(Intent.ACTION_MAIN);
                    i.setComponent(new ComponentName(packageName, row.length > 2 ? row[0] + row[2] : packageName + ".MainActivity"));
                    return i;
                }
            }
            return null;
        }

        @Override
        public Intent getLeanbackLaunchIntentForPackage(String packageName) {
            return null;
        }

        @Override public PackageInfo getPackageInfo(String packageName, int flags) throws NameNotFoundException {
            for (String[] row : apps) {
                if (row[0].equals(packageName)) {
                    PackageInfo pi = new PackageInfo();
                    pi.packageName = packageName;
                    pi.versionName = "1.0.0";
                    return pi;
                }
            }
            throw new NameNotFoundException(packageName);
        }

        @Override public PackageInfo getPackageInfo( android.content.pm.VersionedPackage vp, int flags) { return new PackageInfo(); }
        @Override public String[] currentToCanonicalPackageNames(String[] names) { return names; }
        @Override public String[] canonicalToCurrentPackageNames(String[] names) { return names; }
        @Override public int[] getPackageGids(String packageName) { return new int[0]; }
        @Override public int[] getPackageGids(String packageName, int flags) { return new int[0]; }
        @Override public int getPackageUid(String packageName, int flags) { return 10100; }
        @Override public android.content.pm.PermissionInfo getPermissionInfo(String name, int flags) { return null; }
        @Override public List<android.content.pm.PermissionInfo> queryPermissionsByGroup(String group, int flags) { return Collections.emptyList(); }
        @Override public android.content.pm.PermissionGroupInfo getPermissionGroupInfo(String name, int flags) { return null; }
        @Override public List<android.content.pm.PermissionGroupInfo> getAllPermissionGroups(int flags) { return Collections.emptyList(); }
        @Override public ApplicationInfo getApplicationInfo(String packageName, int flags) throws NameNotFoundException {
            for (String[] row : apps) {
                if (row[0].equals(packageName)) {
                    ApplicationInfo ai = new ApplicationInfo();
                    ai.packageName = packageName;
                    ai.nonLocalizedLabel = row[1];
                    return ai;
                }
            }
            throw new NameNotFoundException(packageName);
        }
        @Override public ActivityInfo getActivityInfo(ComponentName component, int flags) { return new ActivityInfo(); }
        @Override public ActivityInfo getReceiverInfo(ComponentName component, int flags) { return new ActivityInfo(); }
        @Override public android.content.pm.ServiceInfo getServiceInfo(ComponentName component, int flags) { return null; }
        @Override public android.content.pm.ProviderInfo getProviderInfo(ComponentName component, int flags) { return null; }
        @Override public List<PackageInfo> getInstalledPackages(int flags) { return Collections.emptyList(); }
        @Override public List<PackageInfo> getPackagesHoldingPermissions(String[] permissions, int flags) { return Collections.emptyList(); }
        @Override public int checkPermission(String permName, String pkgName) { return PERMISSION_GRANTED; }
        @Override public boolean isPermissionRevokedByPolicy(String permName, String pkgName) { return false; }
        @Override public boolean addPermission(android.content.pm.PermissionInfo info) { return false; }
        @Override public boolean addPermissionAsync(android.content.pm.PermissionInfo info) { return false; }
        @Override public void removePermission(String name) {}
        @Override public int checkSignatures(String pkg1, String pkg2) { return SIGNATURE_MATCH; }
        @Override public int checkSignatures(int uid1, int uid2) { return SIGNATURE_MATCH; }
        @Override public String[] getPackagesForUid(int uid) { return new String[0]; }
        @Override public String getNameForUid(int uid) { return "uid:" + uid; }
        @Override public List<ApplicationInfo> getInstalledApplications(int flags) {
            List<ApplicationInfo> out = new ArrayList<>();
            for (String[] row : apps) {
                ApplicationInfo ai = new ApplicationInfo();
                ai.packageName = row[0];
                ai.nonLocalizedLabel = row[1];
                out.add(ai);
            }
            return out;
        }
        @Override public boolean isInstantApp() { return false; }
        @Override public boolean isInstantApp(String packageName) { return false; }
        @Override public int getInstantAppCookieMaxBytes() { return 0; }
        @Override public byte[] getInstantAppCookie() { return new byte[0]; }
        @Override public void clearInstantAppCookie() {}
        @Override public void updateInstantAppCookie(byte[] cookie) {}
        @Override public String[] getSystemSharedLibraryNames() { return new String[0]; }
        @Override public List<android.content.pm.SharedLibraryInfo> getSharedLibraries(int flags) { return Collections.emptyList(); }
        @Override public android.content.pm.ChangedPackages getChangedPackages(int sequenceNumber) { return null; }
        @Override public android.content.pm.FeatureInfo[] getSystemAvailableFeatures() { return new android.content.pm.FeatureInfo[0]; }
        @Override public boolean hasSystemFeature(String name) { return true; }
        @Override public boolean hasSystemFeature(String name, int version) { return true; }
        @Override public ResolveInfo resolveActivity(Intent intent, int flags) { return null; }
        @Override public List<ResolveInfo> queryIntentActivityOptions(ComponentName caller, Intent[] specifics, Intent intent, int flags) { return Collections.emptyList(); }
        @Override public List<ResolveInfo> queryBroadcastReceivers(Intent intent, int flags) { return Collections.emptyList(); }
        @Override public ResolveInfo resolveService(Intent intent, int flags) { return null; }
        @Override public List<ResolveInfo> queryIntentServices(Intent intent, int flags) { return Collections.emptyList(); }
        @Override public List<ResolveInfo> queryIntentContentProviders(Intent intent, int flags) { return Collections.emptyList(); }
        @Override public android.content.pm.ProviderInfo resolveContentProvider(String name, int flags) { return null; }
        @Override public List<android.content.pm.ProviderInfo> queryContentProviders(String processName, int uid, int flags) { return Collections.emptyList(); }
        @Override public android.content.pm.InstrumentationInfo getInstrumentationInfo(ComponentName className, int flags) { return null; }
        @Override public List<android.content.pm.InstrumentationInfo> queryInstrumentation(String targetPackage, int flags) { return Collections.emptyList(); }
        @Override public android.graphics.drawable.Drawable getDrawable(String packageName, int resid, ApplicationInfo appInfo) { return null; }
        @Override public android.graphics.drawable.Drawable getActivityIcon(ComponentName activityName) { return null; }
        @Override public android.graphics.drawable.Drawable getActivityIcon(Intent intent) { return null; }
        @Override public android.graphics.drawable.Drawable getActivityBanner(ComponentName activityName) { return null; }
        @Override public android.graphics.drawable.Drawable getActivityBanner(Intent intent) { return null; }
        @Override public android.graphics.drawable.Drawable getDefaultActivityIcon() { return null; }
        @Override public android.graphics.drawable.Drawable getApplicationIcon(ApplicationInfo info) { return null; }
        @Override public android.graphics.drawable.Drawable getApplicationIcon(String packageName) { return null; }
        @Override public android.graphics.drawable.Drawable getApplicationBanner(ApplicationInfo info) { return null; }
        @Override public android.graphics.drawable.Drawable getApplicationBanner(String packageName) { return null; }
        @Override public android.graphics.drawable.Drawable getActivityLogo(ComponentName activityName) { return null; }
        @Override public android.graphics.drawable.Drawable getActivityLogo(Intent intent) { return null; }
        @Override public android.graphics.drawable.Drawable getApplicationLogo(ApplicationInfo info) { return null; }
        @Override public android.graphics.drawable.Drawable getApplicationLogo(String packageName) { return null; }
        @Override public android.graphics.drawable.Drawable getUserBadgedIcon(android.graphics.drawable.Drawable icon, android.os.UserHandle user) { return icon; }
        @Override public android.graphics.drawable.Drawable getUserBadgedDrawableForDensity(android.graphics.drawable.Drawable drawable, android.os.UserHandle user, android.graphics.Rect badgeLocation, int badgeDensity) { return drawable; }
        @Override public CharSequence getUserBadgedLabel(CharSequence label, android.os.UserHandle user) { return label; }
        @Override public CharSequence getText(String packageName, int resid, ApplicationInfo appInfo) { return ""; }
        @Override public android.content.res.XmlResourceParser getXml(String packageName, int resid, ApplicationInfo appInfo) { return null; }
        @Override public android.content.res.Resources getResourcesForActivity(ComponentName activityName) { return null; }
        @Override public android.content.res.Resources getResourcesForApplication(ApplicationInfo app) { return null; }
        @Override public android.content.res.Resources getResourcesForApplication(String appPackageName) { return null; }
        @Override public void verifyPendingInstall(int id, int verificationCode) {}
        @Override public void extendVerificationTimeout(int id, int verificationCodeAtTimeout, long millisecondsToDelay) {}
        @Override public void setInstallerPackageName(String targetPackage, String installerPackageName) {}
        @Override public String getInstallerPackageName(String packageName) { return null; }
        @Override public void addPackageToPreferred(String packageName) {}
        @Override public void removePackageFromPreferred(String packageName) {}
        @Override public List<PackageInfo> getPreferredPackages(int flags) { return Collections.emptyList(); }
        @Override public void addPreferredActivity(android.content.IntentFilter filter, int match, ComponentName[] set, ComponentName activity) {}
        @Override public void clearPackagePreferredActivities(String packageName) {}
        @Override public int getPreferredActivities(List<android.content.IntentFilter> outFilters, List<ComponentName> outActivities, String packageName) { return 0; }
        @Override public void setComponentEnabledSetting(ComponentName componentName, int newState, int flags) {}
        @Override public int getComponentEnabledSetting(ComponentName componentName) { return COMPONENT_ENABLED_STATE_DEFAULT; }
        @Override public void setApplicationEnabledSetting(String packageName, int newState, int flags) {}
        @Override public int getApplicationEnabledSetting(String packageName) { return COMPONENT_ENABLED_STATE_DEFAULT; }
        @Override public boolean isSafeMode() { return false; }
        @Override public void setApplicationCategoryHint(String packageName, int categoryHint) {}
        @Override public android.content.pm.PackageInstaller getPackageInstaller() { return null; }
        @Override public boolean canRequestPackageInstalls() { return true; }
    }

    public static final class InMemoryPrefs implements SharedPreferences {
        private final Map<String, Object> data = new ConcurrentHashMap<>();
        private final Set<OnSharedPreferenceChangeListener> listeners = ConcurrentHashMap.newKeySet();

        @Override
        public Map<String, ?> getAll() {
            return new HashMap<>(data);
        }

        @Override
        public String getString(String key, String defValue) {
            Object v = data.get(key);
            return v instanceof String ? (String) v : defValue;
        }

        @Override
        @SuppressWarnings("unchecked")
        public Set<String> getStringSet(String key, Set<String> defValues) {
            Object v = data.get(key);
            return v instanceof Set ? new HashSet<>((Set<String>) v) : defValues;
        }

        @Override
        public int getInt(String key, int defValue) {
            Object v = data.get(key);
            return v instanceof Number ? ((Number) v).intValue() : defValue;
        }

        @Override
        public long getLong(String key, long defValue) {
            Object v = data.get(key);
            return v instanceof Number ? ((Number) v).longValue() : defValue;
        }

        @Override
        public float getFloat(String key, float defValue) {
            Object v = data.get(key);
            return v instanceof Number ? ((Number) v).floatValue() : defValue;
        }

        @Override
        public boolean getBoolean(String key, boolean defValue) {
            Object v = data.get(key);
            return v instanceof Boolean ? (Boolean) v : defValue;
        }

        @Override
        public boolean contains(String key) {
            return data.containsKey(key);
        }

        @Override
        public Editor edit() {
            return new EditorImpl();
        }

        @Override
        public void registerOnSharedPreferenceChangeListener(OnSharedPreferenceChangeListener listener) {
            if (listener != null) listeners.add(listener);
        }

        @Override
        public void unregisterOnSharedPreferenceChangeListener(OnSharedPreferenceChangeListener listener) {
            if (listener != null) listeners.remove(listener);
        }

        private final class EditorImpl implements Editor {
            private final Map<String, Object> updates = new HashMap<>();
            private final Set<String> removals = new HashSet<>();
            private boolean clearAll = false;

            @Override
            public Editor putString(String key, String value) {
                updates.put(key, value);
                return this;
            }

            @Override
            public Editor putStringSet(String key, Set<String> values) {
                updates.put(key, values == null ? null : new HashSet<>(values));
                return this;
            }

            @Override
            public Editor putInt(String key, int value) {
                updates.put(key, value);
                return this;
            }

            @Override
            public Editor putLong(String key, long value) {
                updates.put(key, value);
                return this;
            }

            @Override
            public Editor putFloat(String key, float value) {
                updates.put(key, value);
                return this;
            }

            @Override
            public Editor putBoolean(String key, boolean value) {
                updates.put(key, value);
                return this;
            }

            @Override
            public Editor remove(String key) {
                removals.add(key);
                return this;
            }

            @Override
            public Editor clear() {
                clearAll = true;
                return this;
            }

            @Override
            public boolean commit() {
                apply();
                return true;
            }

            @Override
            public void apply() {
                if (clearAll) {
                    data.clear();
                }
                for (String k : removals) {
                    data.remove(k);
                }
                for (Map.Entry<String, Object> e : updates.entrySet()) {
                    if (e.getValue() == null) {
                        data.remove(e.getKey());
                    } else {
                        data.put(e.getKey(), e.getValue());
                    }
                    for (OnSharedPreferenceChangeListener l : listeners) {
                        l.onSharedPreferenceChanged(InMemoryPrefs.this, e.getKey());
                    }
                }
            }
        }
    }
}
