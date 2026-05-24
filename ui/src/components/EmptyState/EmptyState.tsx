import styles from "./EmptyState.module.css";

interface Props {
  onOpen: () => void;
  errorMessage?: string;
}

export function EmptyState({ onOpen, errorMessage }: Props) {
  return (
    <div className={styles.root}>
      <div className={styles.card}>
        <div className={styles.eyebrow}>i18n-harness</div>
        <h1 className={styles.title}>Open a Qt Linguist catalog</h1>
        <p className={styles.body}>
          Point the harness at a <code>.ts</code> file. The harness reads the
          structure with byte-stable fidelity — the model will translate the
          text; everything around it is deterministic Rust.
        </p>

        <div className={styles.actions}>
          <button type="button" className={styles.primary} onClick={onOpen}>
            Open file…
            <kbd>⌘O</kbd>
          </button>
        </div>

        {errorMessage && (
          <div className={styles.error} role="alert">
            {errorMessage}
          </div>
        )}

        <dl className={styles.tips}>
          <div className={styles.tip}>
            <dt>Round-trip</dt>
            <dd>
              Apply &nbsp;<code>extract → apply</code>&nbsp; with zero edits;
              bytes match on disk.
            </dd>
          </div>
          <div className={styles.tip}>
            <dt>Local-first</dt>
            <dd>No API keys, no telemetry — point it at your local model.</dd>
          </div>
          <div className={styles.tip}>
            <dt>Gate</dt>
            <dd>
              CLDR-driven validation refuses to write malformed targets back.
            </dd>
          </div>
        </dl>
      </div>
    </div>
  );
}
