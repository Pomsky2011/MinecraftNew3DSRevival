package io.github.pomsky2011.n3dsr;

import com.terraformersmc.modmenu.api.ConfigScreenFactory;
import com.terraformersmc.modmenu.api.ModMenuApi;

/** Puts {@link OwnershipScreen} behind the mod's config button in Mod Menu, when it's installed. */
public final class ModMenuIntegration implements ModMenuApi {
	@Override
	public ConfigScreenFactory<?> getModConfigScreenFactory() {
		return OwnershipScreen::new;
	}
}
