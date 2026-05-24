import { useCallback, useEffect, useMemo, useState } from "react";
import { TopBar } from "./components/TopBar/TopBar";
import { EmptyState } from "./components/EmptyState/EmptyState";
import { CatalogList } from "./components/CatalogList/CatalogList";
import { UnitEditor } from "./components/UnitEditor/UnitEditor";
import { Inspector } from "./components/Inspector/Inspector";
import { appVersion, openCatalog, pickCatalogFile } from "./lib/tauri";
import type { CatalogResponse, UnitId } from "./lib/types";
import styles from "./App.module.css";

type Filter = "all" | "untranslated" | "proposed" | "finished";

export function App() {
  const [version, setVersion] = useState("0.0.0");
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const [selectedId, setSelectedId] = useState<UnitId | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    appVersion()
      .then(setVersion)
      .catch(() => setVersion("dev"));
  }, []);

  const openFile = useCallback(async () => {
    setError(null);
    let path: string | null;
    try {
      path = await pickCatalogFile();
    } catch (e) {
      setError(`Open dialog failed: ${formatError(e)}`);
      return;
    }
    if (!path) return;
    setLoading(true);
    try {
      const response = await openCatalog(path);
      setCatalog(response);
      const first =
        response.units.find((u) => u.state === "untranslated") ??
        response.units[0];
      setSelectedId(first?.id ?? null);
      setFilter("all");
      setSearch("");
    } catch (e) {
      setError(`Could not open ${path}: ${formatError(e)}`);
      setCatalog(null);
      setSelectedId(null);
    } finally {
      setLoading(false);
    }
  }, []);

  // Global Cmd+O / Ctrl+O shortcut.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void openFile();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [openFile]);

  const selectedUnit = useMemo(() => {
    if (!catalog || !selectedId) return null;
    return catalog.units.find((u) => u.id === selectedId) ?? null;
  }, [catalog, selectedId]);

  return (
    <div className={styles.app}>
      <TopBar
        catalogPath={catalog?.path ?? null}
        unitCount={catalog?.unit_count ?? 0}
        version={version}
        onOpen={openFile}
      />
      {catalog ? (
        <div className={styles.body}>
          <CatalogList
            units={catalog.units}
            selectedId={selectedId}
            filter={filter}
            search={search}
            onSelect={setSelectedId}
            onFilterChange={setFilter}
            onSearchChange={setSearch}
          />
          {selectedUnit ? (
            <UnitEditor unit={selectedUnit} />
          ) : (
            <div className={styles.nothing}>
              <p>Select a unit on the left to inspect it.</p>
            </div>
          )}
          {selectedUnit && <Inspector unit={selectedUnit} />}
        </div>
      ) : (
        <EmptyState
          onOpen={openFile}
          {...(error ? { errorMessage: error } : {})}
        />
      )}
      {loading && (
        <div className={styles.loading} aria-live="polite">
          Loading catalog…
        </div>
      )}
    </div>
  );
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}
