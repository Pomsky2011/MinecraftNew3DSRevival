package io.github.pomsky2011.n3dsr;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.command.v2.ClientCommandManager;
import net.fabricmc.fabric.api.client.command.v2.ClientCommandRegistrationCallback;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.network.chat.Component;

/**
 * The mod's entrypoint (DESIGN.md §3.1): wires the three ways to start the ownership check (the
 * config screen, {@code /n3dsr export}, {@code -Dn3dsr.export=true}) to the Rust that does it.
 */
public final class N3dsrExport implements ClientModInitializer {
	private static boolean launchCheckStarted;

	@Override
	public void onInitializeClient() {
		ClientCommandRegistrationCallback.EVENT.register((dispatcher, registries) -> dispatcher.register(
			ClientCommandManager.literal("n3dsr").then(ClientCommandManager.literal("export").executes(context -> {
				var source = context.getSource();
				source.sendFeedback(Component.literal("Asking Mojang to confirm ownership…"));
				OwnershipCheck.run(result -> source.sendFeedback(Component.literal(result.proven()
					? "Confirmed. Proof: " + result.proof() + "\nNext, run: " + result.command()
					: result.problem())));
				return 1;
			}))));

		if (n3dsr_export.n3dsr_export.launch_requested()) {
			// At the title screen the game is signed in and settled; check, then quit.
			ScreenEvents.AFTER_INIT.register((client, screen, width, height) -> {
				if (screen instanceof TitleScreen && !launchCheckStarted) {
					launchCheckStarted = true;
					OwnershipCheck.run(result -> client.stop());
				}
			});
		}
	}
}
