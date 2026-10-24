package io.github.pomsky2011.n3dsr.vectors;

import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.NativeImage;
import java.awt.image.BufferedImage;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Locale;
import java.util.zip.ZipFile;
import javax.imageio.ImageIO;
import net.minecraft.client.renderer.texture.MipmapGenerator;
import net.minecraft.client.renderer.texture.MipmapStrategy;
import net.minecraft.resources.Identifier;

/**
 * Parity vectors for n3dsr-assets' mipmapping (DESIGN.md §11.2): runs the game's own
 * MipmapGenerator on textures from the player's client jar, every strategy, and writes each level
 * as a PNG, with the input beside them, for crates/assets/tests/mipmap_vanilla.rs to compare.
 * Development only; its output is Mojang's textures, so it goes in the gitignored assets/ folder.
 *
 * Run from mod/: ./gradlew mipmapVectors
 */
public final class MipmapVectors {
	/** Cutout, translucent, opaque, animated and item textures: every path through the generator. */
	private static final List<String> SPRITES = List.of(
		"block/oak_leaves", "block/vine", "block/short_grass", "block/glass", "block/cobweb", "block/stone",
		"block/grass_block_side_overlay", "block/water_still", "block/sculk", "block/redstone_dust_dot",
		"block/torch", "item/diamond", "item/oak_door"
	);
	private static final int LEVELS = 4;

	public static void main(String[] args) throws Exception {
		Path proof = Path.of(args[0]);
		Path out = Path.of(args[1]);
		String jar = JsonParser.parseString(Files.readString(proof)).getAsJsonObject()
			.getAsJsonObject("client_jar").get("path").getAsString();
		try (ZipFile zip = new ZipFile(jar)) {
			for (String sprite : SPRITES) {
				byte[] source;
				try (InputStream stream = zip.getInputStream(zip.getEntry("assets/minecraft/textures/" + sprite + ".png"))) {
					source = stream.readAllBytes();
				}
				Path dir = out.resolve(sprite);
				Files.createDirectories(dir);
				// The jar's own bytes: re-encoding through ImageIO would brighten greyscale PNGs, which it
				// reads as linear grey; the game reads them with STB, as here, which doesn't.
				Files.write(dir.resolve("input.png"), source);
				for (MipmapStrategy strategy : MipmapStrategy.values()) {
					NativeImage image = NativeImage.read(source);
					NativeImage[] levels = MipmapGenerator.generateMipLevels(
						Identifier.withDefaultNamespace(sprite), new NativeImage[] {image}, LEVELS, strategy, 0.0F
					);
					Path strategyDir = dir.resolve(strategy.getSerializedName());
					Files.createDirectories(strategyDir);
					for (int level = 0; level < levels.length; level++) {
						NativeImage mip = levels[level];
						BufferedImage png = new BufferedImage(mip.getWidth(), mip.getHeight(), BufferedImage.TYPE_INT_ARGB);
						for (int y = 0; y < mip.getHeight(); y++) {
							for (int x = 0; x < mip.getWidth(); x++) {
								png.setRGB(x, y, mip.getPixel(x, y));
							}
						}
						ImageIO.write(png, "png", strategyDir.resolve(level + ".png").toFile());
					}
				}
				System.out.println(String.format(Locale.ROOT, "%s: %d strategies × %d levels", sprite, MipmapStrategy.values().length, LEVELS + 1));
			}
		}
	}
}
