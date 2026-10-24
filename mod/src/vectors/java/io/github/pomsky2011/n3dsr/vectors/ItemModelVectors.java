package io.github.pomsky2011.n3dsr.vectors;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileSystem;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.TreeSet;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.minecraft.client.model.geom.builders.UVPair;
import net.minecraft.client.renderer.block.model.BakedQuad;
import net.minecraft.client.renderer.block.model.BlockModel;
import net.minecraft.client.renderer.block.model.BlockModelPart;
import net.minecraft.client.renderer.block.model.ItemModelGenerator;
import net.minecraft.client.renderer.block.model.TextureSlots;
import net.minecraft.client.renderer.texture.MissingTextureAtlasSprite;
import net.minecraft.client.renderer.texture.SpriteContents;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.renderer.texture.atlas.SpriteResourceLoader;
import net.minecraft.client.renderer.texture.atlas.SpriteSource;
import net.minecraft.client.renderer.texture.atlas.SpriteSourceList;
import net.minecraft.client.resources.model.BlockModelRotation;
import net.minecraft.client.resources.model.Material;
import net.minecraft.client.resources.model.MissingBlockModel;
import net.minecraft.client.resources.model.ModelBaker;
import net.minecraft.client.resources.model.ModelDebugName;
import net.minecraft.client.resources.model.QuadCollection;
import net.minecraft.client.resources.model.ResolvedModel;
import net.minecraft.client.resources.model.SpriteGetter;
import net.minecraft.client.resources.model.UnbakedModel;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.VanillaPackResourcesBuilder;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import org.joml.Vector3fc;

/**
 * Parity vectors for n3dsr-assets' item models (DESIGN.md §3.7, §11.2): every model the vanilla
 * item definitions use, baked by the game's own code with the game's own items and blocks atlas
 * sprites (gathered by its SpriteSourceList from the player's jar), every quad written out
 * (floats as raw bits) for crates/assets/tests/bake_items_vanilla.rs. Generated item models'
 * quads come out in a different order each run (Java builds them from hash sets), so the test
 * compares those as sets. Development only; the output goes in the gitignored assets/ folder.
 *
 * Run from mod/: ./gradlew itemModelVectors
 */
public final class ItemModelVectors {
	public static void main(String[] args) throws Exception {
		Path proof = Path.of(args[0]);
		Path out = Path.of(args[1]);
		String jar = JsonParser.parseString(Files.readString(proof)).getAsJsonObject()
			.getAsJsonObject("client_jar").get("path").getAsString();

		Map<Identifier, UnbakedModel> files = new HashMap<>();
		Set<String> used = new TreeSet<>();
		try (ZipFile zip = new ZipFile(jar)) {
			for (ZipEntry entry : Collections.list(zip.entries())) {
				String name = entry.getName();
				if (name.startsWith("assets/minecraft/models/") && name.endsWith(".json")) {
					try (Reader reader = new InputStreamReader(zip.getInputStream(entry), StandardCharsets.UTF_8)) {
						String path = name.substring("assets/minecraft/models/".length(), name.length() - ".json".length());
						files.put(Identifier.withDefaultNamespace(path), BlockModel.fromStream(reader));
					}
				} else if (name.startsWith("assets/minecraft/items/") && name.endsWith(".json")) {
					try (Reader reader = new InputStreamReader(zip.getInputStream(entry), StandardCharsets.UTF_8)) {
						collectModels(JsonParser.parseReader(reader), used);
					}
				}
			}
		}

		// The client registers its sprite source types at startup; this doesn't start the client.
		net.minecraft.client.renderer.texture.atlas.SpriteSources.bootstrap();
		Map<Identifier, SpriteContents> items;
		Map<Identifier, SpriteContents> blocks;
		try (FileSystem fs = FileSystems.newFileSystem(Path.of(jar))) {
			var vanilla = new VanillaPackResourcesBuilder().exposeNamespace("minecraft").pushUniversalPath(fs.getPath("/"))
				.build(new PackLocationInfo("vanilla", Component.literal("vanilla"), PackSource.BUILT_IN, Optional.empty()));
			var resources = new MultiPackResourceManager(PackType.CLIENT_RESOURCES, List.of(vanilla));
			items = atlas(resources, "items");
			blocks = atlas(resources, "blocks");
		}

		Baker baker = new Baker(items, blocks);
		try (PrintWriter writer = new PrintWriter(Files.newBufferedWriter(out), false)) {
			writer.println("jar " + jar);
			for (String id : used) {
				ResolvedModel model = resolve(files, Identifier.parse(id), new ArrayList<>());
				TextureSlots slots = model.getTopTextureSlots();
				QuadCollection quads = model.bakeTopGeometry(slots, baker, BlockModelRotation.IDENTITY);
				writer.println(String.format(Locale.ROOT, "@ %s %b %d", id, model.getTopAmbientOcclusion(), quads.getAll().size()));
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
		System.out.println(used.size() + " item models written to " + out);
	}

	/** Every model a `minecraft:model` item model names, anywhere in an item definition. */
	private static void collectModels(JsonElement json, Set<String> out) {
		if (json.isJsonObject()) {
			JsonObject object = json.getAsJsonObject();
			if (object.has("type") && object.get("type").isJsonPrimitive()) {
				String type = object.get("type").getAsString();
				if ((type.equals("model") || type.equals("minecraft:model")) && object.has("model") && object.get("model").isJsonPrimitive()) {
					out.add(Identifier.parse(object.get("model").getAsString()).toString());
				}
			}
			object.entrySet().forEach(e -> collectModels(e.getValue(), out));
		} else if (json.isJsonArray()) {
			json.getAsJsonArray().forEach(e -> collectModels(e, out));
		}
	}

	/** An atlas's sprites as the game gathers them: its sources over the jar, then each loader. */
	private static Map<Identifier, SpriteContents> atlas(MultiPackResourceManager resources, String name) {
		SpriteResourceLoader loader = SpriteResourceLoader.create(Set.of());
		Map<Identifier, SpriteContents> sprites = new HashMap<>();
		for (SpriteSource.Loader source : SpriteSourceList.load(resources, Identifier.withDefaultNamespace(name)).list(resources)) {
			SpriteContents contents = source.get(loader);
			if (contents != null) {
				sprites.put(contents.name(), contents);
			}
		}
		return sprites;
	}

	/** As ModelDiscovery links models, with ItemModelGenerator standing at builtin/generated. */
	private static ResolvedModel resolve(Map<Identifier, UnbakedModel> files, Identifier id, List<Identifier> seen) {
		if (id.equals(ItemModelGenerator.GENERATED_ITEM_MODEL_ID)) {
			return new Resolved(id, new ItemModelGenerator(), null);
		}
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

	/** Sprites as `block_or_item` materials find them: items atlas, then blocks, then missing. */
	private static final class Baker implements ModelBaker, SpriteGetter {
		final Map<TextureAtlasSprite, String> textures = new IdentityHashMap<>();
		private final Map<String, TextureAtlasSprite> byName = new HashMap<>();
		private final Map<Identifier, SpriteContents> items;
		private final Map<Identifier, SpriteContents> blocks;

		Baker(Map<Identifier, SpriteContents> items, Map<Identifier, SpriteContents> blocks) {
			this.items = items;
			this.blocks = blocks;
		}

		private TextureAtlasSprite sprite(String name, SpriteContents contents) {
			return byName.computeIfAbsent(name, n -> {
				TextureAtlasSprite sprite = new TextureAtlasSprite(Identifier.withDefaultNamespace("n3dsr_vector"), contents,
					contents.width(), contents.height(), 0, 0, 0) {};
				textures.put(sprite, n);
				return sprite;
			});
		}

		@Override
		public TextureAtlasSprite get(Material material, ModelDebugName name) {
			SpriteContents contents = items.get(material.texture());
			if (contents == null) {
				contents = blocks.get(material.texture());
			}
			if (contents == null) {
				return sprite("minecraft:missingno", MissingTextureAtlasSprite.create());
			}
			return sprite(material.texture().toString(), contents);
		}

		@Override
		public TextureAtlasSprite reportMissingReference(String reference, ModelDebugName name) {
			return sprite("-", MissingTextureAtlasSprite.create());
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
