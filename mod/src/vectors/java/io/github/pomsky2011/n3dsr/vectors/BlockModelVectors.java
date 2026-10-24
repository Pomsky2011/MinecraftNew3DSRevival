package io.github.pomsky2011.n3dsr.vectors;

import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.serialization.JsonOps;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.minecraft.client.model.geom.builders.UVPair;
import net.minecraft.client.renderer.block.model.BakedQuad;
import net.minecraft.client.renderer.block.model.BlockModel;
import net.minecraft.client.renderer.block.model.BlockModelDefinition;
import net.minecraft.client.renderer.block.model.TextureSlots;
import net.minecraft.client.renderer.block.model.Variant;
import net.minecraft.client.renderer.texture.SpriteContents;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.metadata.animation.FrameSize;
import net.minecraft.client.renderer.block.model.BlockModelPart;
import net.minecraft.client.resources.model.Material;
import net.minecraft.client.resources.model.MissingBlockModel;
import net.minecraft.client.resources.model.ModelBaker;
import net.minecraft.client.resources.model.ModelDebugName;
import net.minecraft.client.resources.model.QuadCollection;
import net.minecraft.client.resources.model.ResolvedModel;
import net.minecraft.client.resources.model.SpriteGetter;
import net.minecraft.client.resources.model.UnbakedModel;
import net.minecraft.core.Direction;
import net.minecraft.resources.Identifier;
import org.joml.Vector3fc;

/**
 * Parity vectors for n3dsr-assets' model baking (DESIGN.md §3.7, §11.2): every model and
 * orientation the vanilla blockstates use, baked by the game's own geometry code, every quad
 * written out (floats as raw bits) for crates/assets/tests/bake_vanilla.rs to compare. Sprites
 * span a whole 16×16 "atlas", so the game's atlas UVs are the sprite-local ones we keep.
 * Development only; the output is Mojang-derived, so it goes in the gitignored assets/ folder.
 *
 * Run from mod/: ./gradlew blockModelVectors
 */
public final class BlockModelVectors {
	public static void main(String[] args) throws Exception {
		Path proof = Path.of(args[0]);
		Path out = Path.of(args[1]);
		String jar = JsonParser.parseString(Files.readString(proof)).getAsJsonObject()
			.getAsJsonObject("client_jar").get("path").getAsString();

		Map<Identifier, UnbakedModel> files = new HashMap<>();
		List<Variant> variants = new ArrayList<>();
		try (ZipFile zip = new ZipFile(jar)) {
			for (ZipEntry entry : Collections.list(zip.entries())) {
				String name = entry.getName();
				if (name.startsWith("assets/minecraft/models/") && name.endsWith(".json")) {
					try (Reader reader = new InputStreamReader(zip.getInputStream(entry), StandardCharsets.UTF_8)) {
						String path = name.substring("assets/minecraft/models/".length(), name.length() - ".json".length());
						files.put(Identifier.withDefaultNamespace(path), BlockModel.fromStream(reader));
					}
				} else if (name.startsWith("assets/minecraft/blockstates/") && name.endsWith(".json")) {
					try (Reader reader = new InputStreamReader(zip.getInputStream(entry), StandardCharsets.UTF_8)) {
						BlockModelDefinition definition = BlockModelDefinition.CODEC
							.parse(JsonOps.INSTANCE, JsonParser.parseReader(reader)).getOrThrow();
						definition.simpleModels().ifPresent(s -> s.models().values().forEach(m -> collect(m, variants)));
						definition.multiPart().ifPresent(p -> p.selectors().forEach(sel -> collect(sel.variant(), variants)));
					}
				}
			}
		}

		Set<Variant> distinct = new LinkedHashSet<>(variants);
		Baker baker = new Baker();
		try (PrintWriter writer = new PrintWriter(Files.newBufferedWriter(out), false)) {
			writer.println("jar " + jar);
			for (Variant variant : distinct) {
				Variant.SimpleModelState state = variant.modelState();
				ResolvedModel model = resolve(files, variant.modelLocation(), new ArrayList<>());
				TextureSlots slots = model.getTopTextureSlots();
				QuadCollection quads = model.bakeTopGeometry(slots, baker, state.asModelState());
				writer.println(String.format(Locale.ROOT, "@ %s %d %d %d %b %b %d", variant.modelLocation(),
					state.x().shift, state.y().shift, state.z().shift, state.uvLock(), model.getTopAmbientOcclusion(), quads.getAll().size()));
				Map<BakedQuad, Direction> cull = new IdentityHashMap<>();
				for (Direction direction : Direction.values()) {
					for (BakedQuad quad : quads.getQuads(direction)) {
						cull.put(quad, direction);
					}
				}
				for (BakedQuad quad : quads.getAll()) {
					StringBuilder line = new StringBuilder();
					line.append(baker.textures.get(quad.sprite())).append(' ').append(quad.tintIndex())
						.append(' ').append(quad.direction().getSerializedName())
						.append(' ').append(cull.containsKey(quad) ? cull.get(quad).getSerializedName() : "-")
						.append(' ').append(quad.shade()).append(' ').append(quad.lightEmission());
					for (Vector3fc p : List.of(quad.position0(), quad.position1(), quad.position2(), quad.position3())) {
						line.append(String.format(" %08x %08x %08x",
							Float.floatToRawIntBits(p.x()), Float.floatToRawIntBits(p.y()), Float.floatToRawIntBits(p.z())));
					}
					for (long uv : new long[] {quad.packedUV0(), quad.packedUV1(), quad.packedUV2(), quad.packedUV3()}) {
						line.append(String.format(" %08x %08x",
							Float.floatToRawIntBits(UVPair.unpackU(uv)), Float.floatToRawIntBits(UVPair.unpackV(uv))));
					}
					writer.println(line);
				}
			}
		}
		System.out.println(distinct.size() + " model orientations written to " + out);
	}

	private static void collect(net.minecraft.client.renderer.block.model.BlockStateModel.Unbaked model, List<Variant> out) {
		switch (model) {
			case net.minecraft.client.renderer.block.model.SingleVariant.Unbaked single -> out.add(single.variant());
			case net.minecraft.client.resources.model.WeightedVariants.Unbaked weighted -> weighted.entries().unwrap()
				.forEach(w -> collect(w.value(), out));
			default -> throw new IllegalStateException("unexpected " + model);
		}
	}

	/** The model and its parents, as ModelDiscovery links them: the missing model for a missing one or a cycle. */
	private static ResolvedModel resolve(Map<Identifier, UnbakedModel> files, Identifier id, List<Identifier> seen) {
		UnbakedModel model = files.get(id);
		if (model == null || seen.contains(id)) {
			return new Resolved(MissingBlockModel.LOCATION, MissingBlockModel.missingModel(), null);
		}
		seen.add(id);
		ResolvedModel parent = model.parent() == null ? null : resolve(files, model.parent(), seen);
		return new Resolved(id, model, parent);
	}

	private record Resolved(Identifier id, UnbakedModel wrapped, ResolvedModel parent) implements ResolvedModel {
		@Override
		public String debugName() {
			return id.toString();
		}
	}

	/** Hands out one whole-atlas sprite per texture, and nothing else. */
	private static final class Baker implements ModelBaker, SpriteGetter {
		final Map<TextureAtlasSprite, String> textures = new IdentityHashMap<>();
		private final Map<String, TextureAtlasSprite> byName = new HashMap<>();

		private TextureAtlasSprite sprite(String name) {
			return byName.computeIfAbsent(name, n -> {
				SpriteContents contents = new SpriteContents(Identifier.withDefaultNamespace("n3dsr_vector"), new FrameSize(16, 16), new NativeImage(16, 16, false));
				TextureAtlasSprite sprite = new TextureAtlasSprite(Identifier.withDefaultNamespace("n3dsr_vector"), contents, 16, 16, 0, 0, 0) {};
				textures.put(sprite, n);
				return sprite;
			});
		}

		@Override
		public TextureAtlasSprite get(Material material, ModelDebugName name) {
			return sprite(material.texture().toString());
		}

		@Override
		public TextureAtlasSprite reportMissingReference(String reference, ModelDebugName name) {
			return sprite("-");
		}

		@Override
		public SpriteGetter sprites() {
			return this;
		}

		@Override
		public ModelBaker.PartCache parts() {
			return vector -> vector;
		}

		@Override
		public ResolvedModel getModel(Identifier identifier) {
			throw new UnsupportedOperationException();
		}

		@Override
		public BlockModelPart missingBlockModelPart() {
			throw new UnsupportedOperationException();
		}

		@Override
		public <T> T compute(ModelBaker.SharedOperationKey<T> key) {
			throw new UnsupportedOperationException();
		}
	}
}
