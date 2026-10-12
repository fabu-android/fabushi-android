# Android Remote Runner replacement contract

Authority lock: \`bhrumom/fabushi-desktop@3bc92400826cc4ca7ac665b467708e22261edc61\`.

## Disposition

The **Desktop box-exec daemon listener itself** is \`not-applicable-with-replacement\` inside the Android APK/AAB. This disposition does **not** remove the user capability.

The preserved user capability is: Agent work that requires a desktop OS execution environment (bounded shell/read, Computer Use, or other executor-specific operations) remains available through an explicitly paired and authorized **external Remote Runner**. Android is the authenticated client and product UI; it is not the server that performs arbitrary desktop-OS side effects.

## Source facts

Desktop \`source/box-exec-daemon/src/main.rs\` starts a token-authenticated box-local server and owns its shutdown lifecycle. Desktop Host computer execution is separately gated by real injected executor resources and monitor ownership; tool names alone do not create execution capability.

Android intentionally reuses part of the Remote execution crate without adopting that deployment shape:

- \`source/host/Cargo.toml\` **does** depend transitively on \`fabushi-android-box-exec-daemon\`; the shipping Host uses its \`AuthenticatedRemoteHttpTransport\` client types from \`remote_routed_tools.rs\`.
- Android \`source/box-exec-daemon/Cargo.toml\` defines a library target and no daemon \`[[bin]]\` target.
- shipping Host/JNI code does not reference \`RemoteExecutionService\`, \`server::\`, a socket listener, or a box-exec entrypoint.
- Android Full CI packages only the \`fabushi-android-host-jni\` cdylib into \`jniLibs\` for \`arm64-v8a\` and \`x86_64\`; it does not package a box-exec daemon executable.
- \`mobile/android/app/build.gradle\` does not launch or package a box-exec listener.

So the crate is part of the native dependency closure as a **client transport / protocol and reference service library**, while a Desktop-style server/listener is not part of the Android application composition.

These facts are enforced by \`tests/test_remote_runner_deployment_contract.py\` and by the Rust contract in \`source/box-exec-daemon/src/deployment_contract.rs\`.

## Replacement security boundary

Android may expose a remote executor only when the canonical Host has a protected binding whose account fence, account epoch, device identity and **specific executor set** still match the current account. Approval is consumed once at the Host capability boundary before a side effect crosses to the Remote Runner.

The external Remote Runner must independently authenticate the request identity and credential. Execute/reconcile/cancel remain bound to the same operation/request/account/grant/device identity. A transport timeout or process death after dispatch is \`outcome-unknown\`; it is reconciled and never blindly replayed.

The \`/v1/computers\` control-plane credentials are intentionally separate:

- \`clientToken\` pairs a mobile client and may create a control session.
- \`mobileToken\` authorizes that paired mobile actor only inside an activated control session.
- \`deviceSecret\` belongs to the remote-computer target.
- none of these credentials is accepted as evidence for \`RemoteDispatchBinding.bearerCredential\`.

Until a real external Runner enrollment/credential source establishes that outbound binding, Host executor exposure remains fail-closed.

## User-visible platform delta

Desktop can host its own local box-exec listener because it owns a desktop execution environment. A normal Android application does not gain arbitrary desktop-OS shell, filesystem or pointer control. The Android product shows the remote target and routes only explicitly available executor capabilities to an authorized Remote Runner. Local Android app actions continue through typed Android/App Surface capabilities.

## Verification requirements

This contract may be considered **implemented** when the source contract and tests exist. It must not be promoted to **verified** until the same exact PR HEAD passes:

1. Rust workspace tests, including the deployment contract;
2. architecture checker tests proving the Android Host consumes only the intended Remote client transport and does not compose/package a listener;
3. executor-specific Host gating and protected-binding tests;
4. packaged/device acceptance proving the Android product can use an actually authorized Remote Runner without a hidden in-APK listener.

The final migration gate still requires \`strict_incomplete_rows=0\`; this contract must not be used to bulk-mark unrelated Remote/Computer responsibilities verified.
