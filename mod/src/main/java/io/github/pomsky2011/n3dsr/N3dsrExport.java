package io.github.pomsky2011.n3dsr;

import net.fabricmc.api.ClientModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** The mod's entrypoint: Fabric needs a Java class of this shape; the work is in Rust. */
public final class N3dsrExport implements ClientModInitializer {
	private static final Logger LOGGER = LoggerFactory.getLogger("n3dsr-export");

	@Override
	public void onInitializeClient() {
		LOGGER.info("{} blocks registered (counted in Rust)", n3dsr_export.n3dsr_export.block_count());
	}
}
