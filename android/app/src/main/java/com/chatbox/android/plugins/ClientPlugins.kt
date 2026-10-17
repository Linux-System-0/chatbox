package com.chatbox.android.plugins

/**
 * Client plugins.
 *
 * Language packs and themes are installed and applied entirely on the client;
 * they never change server behavior (see docs/plugin-system.md).
 */
object ClientPlugins {
	data class Language(val code: String, val label: String)

	val languages = listOf(
		Language("en", "English"),
		Language("zh-CN", "简体中文"),
	)

	val themes = listOf("system", "light", "dark")

	private val strings: Map<String, Map<String, String>> = mapOf(
		"en" to mapOf(
			"app_name" to "ChatBox",
			"login" to "Sign in",
			"register" to "Create account",
			"logout" to "Sign out",
			"user_id" to "User ID",
			"password" to "Password",
			"display_name" to "Display name",
			"send" to "Send",
			"message_placeholder" to "Write a message…",
			"conversations" to "Conversations",
			"new_chat" to "New chat (user id)",
		),
		"zh-CN" to mapOf(
			"app_name" to "ChatBox",
			"login" to "登录",
			"register" to "注册",
			"logout" to "退出登录",
			"user_id" to "用户 ID",
			"password" to "密码",
			"display_name" to "显示名称",
			"send" to "发送",
			"message_placeholder" to "输入消息…",
			"conversations" to "会话",
			"new_chat" to "新会话（用户 ID）",
		),
	)

	fun translate(locale: String, key: String): String =
		strings[locale]?.get(key) ?: strings.getValue("en")[key] ?: key
}
