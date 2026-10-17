// Validate the bundled example client plugins.
//
// A language pack must translate every key in the clients' base dictionaries,
// because a shipped pack with missing keys silently falls back to English.
// Usage: node scripts/check-language-packs.mjs

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const webSource = readFileSync(join(root, 'web/src/lib/i18n.ts'), 'utf8');
const androidSource = readFileSync(
	join(root, 'android/app/src/main/java/com/chatbox/android/plugins/ClientPlugins.kt'),
	'utf8'
);

/** Extract `key: 'value'` entries from the Web base dictionary. */
function webStrings(source) {
	const block = source.match(/const baseStrings: Record<string, string> = \{(.*?)\n\};/s);
	if (!block) throw new Error('could not find the Web base dictionary');
	return new Map(
		[...block[1].matchAll(/^\s*([a-z0-9_]+):\s*'((?:[^'\\]|\\.)*)'/gm)].map((match) => [
			match[1],
			match[2]
		])
	);
}

/** Extract `"key" to "value"` entries from the Android base dictionary. */
function androidStrings(source) {
	const block = source.match(/private val baseStrings = mapOf\((.*?)\n\t\)/s);
	if (!block) throw new Error('could not find the Android base dictionary');
	return new Map(
		[...block[1].matchAll(/^\s*"([a-z0-9_]+)" to\s*"([^"]*)"/gm)].map((match) => [
			match[1],
			match[2]
		])
	);
}

function placeholders(value) {
	return new Set([...value.matchAll(/\{([a-z0-9_]+)\}/gi)].map((match) => match[1]));
}

function comparePlaceholders(file, key, english, translated, errors) {
	const expected = placeholders(english);
	const actual = placeholders(translated);
	for (const name of expected) {
		if (!actual.has(name)) errors.push(`${file}: ${key} is missing placeholder {${name}}`);
	}
	for (const name of actual) {
		if (!expected.has(name)) errors.push(`${file}: ${key} has unexpected placeholder {${name}}`);
	}
}

const base = new Map([...webStrings(webSource), ...androidStrings(androidSource)]);
const pluginDir = join(root, 'examples/plugins');
const errors = [];
const warnings = [];

for (const file of readdirSync(pluginDir).filter((name) => name.endsWith('.json')).sort()) {
	const path = join(pluginDir, file);
	let plugin;
	try {
		plugin = JSON.parse(readFileSync(path, 'utf8'));
	} catch (error) {
		errors.push(`${file}: invalid JSON (${error.message})`);
		continue;
	}
	if (plugin.kind !== 'language-pack') continue;

	const strings = plugin.payload?.strings;
	if (!strings || typeof strings !== 'object' || Array.isArray(strings)) {
		errors.push(`${file}: payload.strings must be an object`);
		continue;
	}
	for (const [key, english] of base) {
		const translated = strings[key];
		if (typeof translated !== 'string' || translated.trim() === '') {
			errors.push(`${file}: missing translation for "${key}"`);
			continue;
		}
		comparePlaceholders(file, key, english, translated, errors);
	}
	for (const key of Object.keys(strings)) {
		if (!base.has(key)) warnings.push(`${file}: unused string key "${key}"`);
	}
}

if (warnings.length > 0) console.warn(warnings.join('\n'));
if (errors.length > 0) {
	console.error(errors.join('\n'));
	process.exit(1);
}
console.log(`checked example language packs against ${base.size} base keys`);
