package io.github.pomsky2011.n3dsr;

import net.minecraft.client.main.GameConfig;

/** The folders the game was started with, kept by {@code MinecraftMixin} for the Rust to read. */
public final class GameFolders {
	public static GameConfig.FolderData folders;

	private GameFolders() {}
}
