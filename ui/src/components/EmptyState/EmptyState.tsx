interface Props {
  onOpen: () => void;
  errorMessage?: string;
}

export function EmptyState({ onOpen, errorMessage }: Props) {
  return (
    <div className="flex-1 flex items-center justify-center px-6 py-12 bg-bg-base overflow-y-auto">
      <div className="w-full max-w-[560px] px-8 py-10 rounded-lg border border-border-subtle bg-bg-surface">
        <div className="mb-3 text-xs font-semibold uppercase tracking-loose text-accent">
          i18n-harness
        </div>
        <h1 className="m-0 mb-3 text-2xl font-semibold tracking-tight leading-tight text-fg-primary">
          Open a Qt Linguist catalog
        </h1>
        <p className="m-0 mb-6 text-md text-fg-secondary leading-[1.65]">
          Point the harness at a <CodeChip>.ts</CodeChip> file. The harness
          reads the structure with byte-stable fidelity — the model will
          translate the text; everything around it is deterministic Rust.
        </p>

        <div className="flex gap-3 mb-8">
          <button
            type="button"
            onClick={onOpen}
            className="inline-flex items-center gap-3 h-9 px-4 rounded-md text-md font-medium text-accent-fg bg-accent transition-colors duration-100 ease-out hover:bg-accent-hover active:bg-accent-active active:translate-y-px"
          >
            Open file…
            <kbd>⌘O</kbd>
          </button>
        </div>

        {errorMessage && (
          <div
            role="alert"
            className="mb-6 px-4 py-3 rounded-md border border-severity-hard-border bg-severity-hard-bg text-severity-hard text-sm font-mono whitespace-pre-wrap break-words"
          >
            {errorMessage}
          </div>
        )}

        <dl className="grid gap-4 pt-6 border-t border-border-subtle">
          <Tip term="Round-trip">
            Apply <CodeChip>extract → apply</CodeChip> with zero edits; bytes
            match on disk.
          </Tip>
          <Tip term="Local-first">
            No API keys, no telemetry — point it at your local model.
          </Tip>
          <Tip term="Gate">
            CLDR-driven validation refuses to write malformed targets back.
          </Tip>
        </dl>
      </div>
    </div>
  );
}

function Tip({ term, children }: { term: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[110px_1fr] gap-4 items-baseline">
      <dt className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
        {term}
      </dt>
      <dd className="m-0 text-sm text-fg-secondary leading-snug">{children}</dd>
    </div>
  );
}

function CodeChip({ children }: { children: React.ReactNode }) {
  return (
    <code className="px-1 py-px font-mono text-[0.9em] bg-bg-elevated border border-border-subtle rounded-sm text-fg-primary">
      {children}
    </code>
  );
}
