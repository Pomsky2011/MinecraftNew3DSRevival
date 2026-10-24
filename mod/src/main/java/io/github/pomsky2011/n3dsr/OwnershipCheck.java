package io.github.pomsky2011.n3dsr;

import com.mojang.authlib.exceptions.AuthenticationException;
import java.util.concurrent.CompletableFuture;
import java.util.function.Consumer;
import n3dsr_export.Outcome;
import net.minecraft.client.Minecraft;
import net.minecraft.util.Util;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** Runs the Rust's ownership check off the render thread and reports back on it. */
public final class OwnershipCheck {
	static final Logger LOGGER = LoggerFactory.getLogger("n3dsr-export");

	/** The check's result: the proof's path, or why there isn't one. */
	public record Result(String proof, String problem) {
		public boolean proven() {
			return proof != null;
		}

		public String command() {
			return n3dsr_export.n3dsr_export.command_for(proof);
		}
	}

	private OwnershipCheck() {}

	public static void run(Consumer<Result> done) {
		CompletableFuture.supplyAsync(OwnershipCheck::check, Util.backgroundExecutor())
			.thenAcceptAsync(result -> {
				if (result.proven()) {
					LOGGER.info("Ownership confirmed; proof written to {}", result.proof());
				} else {
					LOGGER.warn("No proof written: {}", result.problem());
				}
				done.accept(result);
			}, Minecraft.getInstance());
	}

	private static Result check() {
		try {
			Outcome outcome = n3dsr_export.n3dsr_export.check();
			if (outcome instanceof Outcome.Proven proven) {
				return new Result(proven.value, null);
			}
			return new Result(null, ((Outcome.Refused) outcome).value);
		} catch (Throwable e) {
			// Rust declares no checked exceptions, so javac won't let authlib's be caught by type.
			if (e instanceof AuthenticationException) {
				// An offline or expired session, or Mojang unreachable.
				LOGGER.warn("Mojang refused the session", e);
				return new Result(null, "Mojang refused the session (" + e.getMessage()
					+ "). Launch the game signed in, through your launcher, and try again.");
			}
			LOGGER.error("Ownership check failed", e);
			return new Result(null, "The check failed with an error of the mod's own (" + e
				+ "); the game log has the details.");
		}
	}
}
