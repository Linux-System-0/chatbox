// Guard the Android client's runtime network policy.
//
// The policy only works when every request goes through the shared OkHttp
// client and the manifest lets the app (not the platform) enforce cleartext.
// This script fails on a direct OkHttpClient construction, on a compile-time
// network security config, and on the old BuildConfig server URL. When an APK
// is available (or --require-apk is passed), it also inspects the packaged
// manifest and resources.
//
// Usage: node scripts/check-android-network-policy.mjs [--require-apk]

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const android = join(root, 'android');
const sourceRoot = join(android, 'app/src/main/java');
const factory = join(sourceRoot, 'com/chatbox/android/network/NetworkRuntime.kt');
const manifest = join(android, 'app/src/main/AndroidManifest.xml');
const removedConfig = join(android, 'app/src/main/res/xml/network_security_config.xml');
const apk = join(android, 'app/build/outputs/apk/debug/app-debug.apk');
const requireApk = process.argv.includes('--require-apk');
const errors = [];

function walk(directory) {
	return readdirSync(directory).flatMap((name) => {
		const path = join(directory, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});
}

if (!existsSync(factory)) {
	errors.push('the shared OkHttp client factory is missing');
} else {
	for (const path of walk(sourceRoot).filter((path) => path.endsWith('.kt'))) {
		if (path === factory) continue;
		const source = readFileSync(path, 'utf8');
		if (/\bOkHttpClient\s*\(/.test(source) || /\bOkHttpClient\s*\.\s*Builder\s*\(/.test(source)) {
			errors.push(`${path}: constructs OkHttpClient directly; use NetworkClientFactory`);
		}
	}
}

if (!existsSync(manifest)) {
	errors.push('app/src/main/AndroidManifest.xml is missing');
} else {
	const text = readFileSync(manifest, 'utf8');
	if (!/android:usesCleartextTraffic\s*=\s*"true"/.test(text)) {
		errors.push('AndroidManifest.xml must set android:usesCleartextTraffic="true"');
	}
	if (!/android:allowBackup\s*=\s*"false"/.test(text)) {
		errors.push('AndroidManifest.xml must set android:allowBackup="false"');
	}
	if (/android:networkSecurityConfig\s*=/.test(text)) {
		errors.push('AndroidManifest.xml must not reference android:networkSecurityConfig');
	}
}

if (existsSync(removedConfig)) {
	errors.push('res/xml/network_security_config.xml must be deleted');
}

const gradle = readFileSync(join(android, 'app/build.gradle.kts'), 'utf8');
if (/API_BASE_URL/.test(gradle)) {
	errors.push('app/build.gradle.kts must not define BuildConfig.API_BASE_URL');
}
if (existsSync(sourceRoot)) {
	for (const path of walk(sourceRoot).filter((path) => path.endsWith('.kt'))) {
		if (readFileSync(path, 'utf8').includes('BuildConfig.API_BASE_URL')) {
			errors.push(`${path}: uses BuildConfig.API_BASE_URL`);
		}
	}
}

function findAapt2() {
	const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
	if (!sdk) return null;
	const buildTools = join(sdk, 'build-tools');
	if (!existsSync(buildTools)) return null;
	const versions = readdirSync(buildTools).sort().reverse();
	for (const version of versions) {
		const candidate = join(buildTools, version, 'aapt2');
		if (existsSync(candidate)) return candidate;
	}
	return null;
}

if (existsSync(apk)) {
	const aapt2 = findAapt2();
	if (!aapt2) {
		errors.push('cannot inspect the APK: aapt2 was not found (set ANDROID_HOME)');
	} else {
		const xml = execFileSync(aapt2, ['dump', 'xmltree', apk, '--file', 'AndroidManifest.xml'], {
			encoding: 'utf8'
		});
		if (!/usesCleartextTraffic[^\n]*=\s*true/.test(xml)) {
			errors.push('the packaged manifest does not enable usesCleartextTraffic');
		}
		if (/networkSecurityConfig/.test(xml)) {
			errors.push('the packaged manifest still references a network security config');
		}
		const resources = execFileSync(aapt2, ['dump', 'resources', apk], { encoding: 'utf8' });
		if (resources.includes('network_security_config')) {
			errors.push('the APK still contains the network_security_config resource');
		}
	}
} else if (requireApk) {
	errors.push(`the APK is missing: ${apk}`);
}

if (errors.length > 0) {
	console.error(errors.join('\n'));
	process.exit(1);
}
console.log('android network policy guard passed');
