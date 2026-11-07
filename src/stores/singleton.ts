/**
 * Store singleton registry.
 *
 * In dev, Vite may evaluate a module twice (an HMR update, or the same file
 * requested once with a `?t=` timestamp and once without). Every evaluation of
 * `create(...)` builds a brand new zustand store, so the sidebar could end up
 * mutating one instance while `Layout` rendered from another — clicking a
 * sidebar entry changed nothing on screen until the whole dev server was
 * restarted.
 *
 * Registering the store on `window` makes all evaluations share one instance,
 * so state never splits. Production only ever evaluates each module once, so
 * this is a dev-time guard and stays out of the way there (and in tests, where
 * `vi.resetModules()` deliberately expects a fresh store).
 */

const REGISTRY_KEY = "__pebbleStoreRegistry";

/** HMR + duplicate module URLs only happen in dev; tests need fresh stores. */
const SHARE_STORES = import.meta.env.DEV && import.meta.env.MODE !== "test";

type Registry = Record<string, unknown>;

function getRegistry(): Registry | null {
  if (!SHARE_STORES || typeof window === "undefined") return null;
  const host = window as unknown as Record<string, Registry | undefined>;
  if (!host[REGISTRY_KEY]) host[REGISTRY_KEY] = {};
  return host[REGISTRY_KEY] as Registry;
}

export function createSingleton<T>(key: string, factory: () => T): T {
  const registry = getRegistry();
  if (!registry) return factory();

  if (key in registry) {
    if (import.meta.env.DEV) {
      // eslint-disable-next-line no-console
      console.debug(
        `[stores] "${key}" was evaluated twice; reusing the existing instance.`,
      );
    }
    return registry[key] as T;
  }

  const instance = factory();
  registry[key] = instance;
  return instance;
}
