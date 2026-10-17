/** Profile display helpers and client-side avatar preparation. */

/** Name precedence: contact remark, then display name, then custom ID. */
export function effectiveName(
	remark: string | null | undefined,
	displayName: string | null | undefined,
	userId: string
): string {
	const remarkText = remark?.trim();
	if (remarkText) return remarkText;
	const displayText = displayName?.trim();
	if (displayText) return displayText;
	return userId;
}

/** Avatar precedence: contact remark avatar, then profile avatar. */
export function effectiveAvatar(
	remarkAvatar: string | null | undefined,
	profileAvatar: string | null | undefined
): string | null {
	const remark = remarkAvatar?.trim();
	if (remark) return remark;
	const profile = profileAvatar?.trim();
	if (profile) return profile;
	return null;
}

const MAX_AVATAR_CHARS = 700_000;

/**
 * Read an image file into a downscaled JPEG data URL.
 *
 * The backend accepts avatars up to 512 KiB; downscaling keeps even large
 * phone photos far below that limit while preserving a crisp small avatar.
 */
export async function fileToAvatar(file: File, maxSize = 256): Promise<string> {
	if (!file.type.startsWith('image/')) {
		throw new Error('choose an image file');
	}
	const bitmap = await loadBitmap(file);
	const scale = Math.min(1, maxSize / Math.max(bitmap.width, bitmap.height));
	const width = Math.max(1, Math.round(bitmap.width * scale));
	const height = Math.max(1, Math.round(bitmap.height * scale));
	const canvas = document.createElement('canvas');
	canvas.width = width;
	canvas.height = height;
	const context = canvas.getContext('2d');
	if (!context) {
		bitmap.close?.();
		throw new Error('cannot process the image');
	}
	context.drawImage(bitmap, 0, 0, width, height);
	bitmap.close?.();
	const dataUrl = canvas.toDataURL('image/jpeg', 0.84);
	if (dataUrl.length > MAX_AVATAR_CHARS) {
		throw new Error('image is too large');
	}
	return dataUrl;
}

async function loadBitmap(file: File): Promise<ImageBitmap> {
	if (typeof createImageBitmap === 'function') {
		return await createImageBitmap(file);
	}
	const url = URL.createObjectURL(file);
	try {
		const image = new Image();
		image.src = url;
		await image.decode();
		return await createImageBitmap(image);
	} finally {
		URL.revokeObjectURL(url);
	}
}
