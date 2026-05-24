import styles from "./TopBar.module.css";

interface Props {
  catalogPath: string | null;
  unitCount: number;
  version: string;
  onOpen: () => void;
}

export function TopBar({ catalogPath, unitCount, version, onOpen }: Props) {
  return (
    <header className={`${styles.bar} app-chrome`}>
      <div className={styles.brand}>
        <span className={styles.brandMark} aria-hidden="true" />
        <span className={styles.brandName}>i18n-harness</span>
        <span className={styles.version}>v{version}</span>
      </div>

      <div className={styles.actions}>
        <button
          type="button"
          className={styles.action}
          onClick={onOpen}
          aria-label="Open catalog"
          title="Open a .ts catalog (⌘O)"
        >
          <span aria-hidden="true" className={styles.actionIcon}>
            ⌥
          </span>
          Open
          <kbd>⌘O</kbd>
        </button>
        <button
          type="button"
          className={styles.action}
          disabled
          title="Save — wired in the next milestone"
        >
          Save
          <kbd>⌘S</kbd>
        </button>
      </div>

      <div className={styles.meta}>
        {catalogPath ? (
          <>
            <span className={styles.metaPath} title={catalogPath}>
              {shortenPath(catalogPath)}
            </span>
            <span className={styles.metaDivider}>·</span>
            <span className={styles.metaCount}>
              {unitCount} {unitCount === 1 ? "unit" : "units"}
            </span>
          </>
        ) : (
          <span className={styles.metaIdle}>No catalog</span>
        )}
      </div>
    </header>
  );
}

function shortenPath(p: string): string {
  if (p.length <= 60) return p;
  const parts = p.split(/[\\/]/);
  if (parts.length <= 3) return p;
  return `…/${parts.slice(-3).join("/")}`;
}
