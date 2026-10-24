package io.github.pomsky2011.n3dsr;

import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.MultiLineTextWidget;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;

/** The config screen: nothing reaches Mojang until "Check ownership" is pressed. */
public final class OwnershipScreen extends Screen {
	private static final int TEXT_WIDTH = 300;

	private final Screen parent;
	private Component status = Component.literal(
		"Before the New 3DS build can be made, Mojang has to confirm that this account owns"
			+ " Minecraft: Java Edition, the way an online-mode server does when you join it. This"
			+ " writes a proof file for n3dsr-build.\n\nNot an official Minecraft product; not"
			+ " approved by or associated with Mojang or Microsoft.");
	private boolean checking;
	private OwnershipCheck.Result result;

	public OwnershipScreen(Screen parent) {
		super(Component.literal("Minecraft New 3DS Revival"));
		this.parent = parent;
	}

	@Override
	protected void init() {
		int x = (width - TEXT_WIDTH) / 2;
		addRenderableWidget(new MultiLineTextWidget(x, 40, status, font).setMaxWidth(TEXT_WIDTH).setCentered(true));

		int y = height - 52;
		if (result != null && result.proven()) {
			addRenderableWidget(Button.builder(Component.literal("Copy command"), button ->
				minecraft.keyboardHandler.setClipboard(result.command())).bounds(width / 2 - 155, y, 150, 20).build());
		} else {
			Button check = Button.builder(Component.literal(checking ? "Checking…" : "Check ownership"), button -> start())
				.bounds(width / 2 - 155, y, 150, 20).build();
			check.active = !checking;
			addRenderableWidget(check);
		}
		addRenderableWidget(Button.builder(Component.literal("Done"), button -> onClose())
			.bounds(width / 2 + 5, y, 150, 20).build());
	}

	private void start() {
		checking = true;
		status = Component.literal("Asking Mojang…");
		rebuildWidgets();
		OwnershipCheck.run(done -> {
			checking = false;
			result = done;
			status = Component.literal(done.proven()
				? "Confirmed. Proof written to:\n" + done.proof() + "\n\nNext, run:\n" + done.command()
				: done.problem());
			rebuildWidgets();
		});
	}

	@Override
	public void onClose() {
		minecraft.setScreen(parent);
	}
}
