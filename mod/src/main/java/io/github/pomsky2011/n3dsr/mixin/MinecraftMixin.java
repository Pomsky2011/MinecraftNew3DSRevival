package io.github.pomsky2011.n3dsr.mixin;

import io.github.pomsky2011.n3dsr.GameFolders;
import net.minecraft.client.Minecraft;
import net.minecraft.client.main.GameConfig;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Keeps the game's folders (the asset directory and index among them), which it doesn't keep itself. */
@Mixin(Minecraft.class)
abstract class MinecraftMixin {
	@Inject(method = "<init>", at = @At("RETURN"))
	private void n3dsr$keepFolders(GameConfig config, CallbackInfo ci) {
		GameFolders.folders = config.location;
	}
}
