import { cn } from "../../lib/cn";

interface Props {
  catalogPath: string | null;
  unitCount: number;
  version: string;
  onOpen: () => void;
}

export function TopBar({ catalogPath, unitCount, version, onOpen }: Props) {
  return (
    <header
      className={cn(
        "app-chrome shrink-0 h-11 px-4 grid grid-cols-[1fr_auto_1fr] items-center",
        "bg-bg-surface border-b border-border-subtle",
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        <span
          aria-hidden="true"
          className={cn(
            "shrink-0 w-3.5 h-3.5 rounded-sm border border-accent-subtle-border",
            "bg-gradient-to-br from-accent to-accent-active",
          )}
        />
        <span className="text-md font-semibold tracking-tight text-fg-primary">
          i18n-harness
        </span>
        <span className="font-mono text-xs text-fg-tertiary tracking-loose">
          v{version}
        </span>
      </div>

      <div className="flex items-center gap-2">
        <ActionButton onClick={onOpen} title="Open a .ts catalog (⌘O)">
          Open
          <kbd>⌘O</kbd>
        </ActionButton>
        <ActionButton
          disabled
          title="Save — wired in the next milestone"
        >
          Save
          <kbd>⌘S</kbd>
        </ActionButton>
      </div>

      <div className="flex items-center gap-2 justify-end min-w-0">
        {catalogPath ? (
          <>
            <span
              className="font-mono text-xs text-fg-secondary truncate max-w-[360px]"
              title={catalogPath}
            >
              {shortenPath(catalogPath)}
            </span>
            <span className="text-fg-disabled">·</span>
            <span className="text-xs text-fg-tertiary tracking-loose whitespace-nowrap">
              {unitCount} {unitCount === 1 ? "unit" : "units"}
            </span>
          </>
        ) : (
          <span className="text-xs text-fg-disabled tracking-loose uppercase">
            No catalog
          </span>
        )}
      </div>
    </header>
  );
}

function ActionButton({
  children,
  onClick,
  disabled,
  title,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        "inline-flex items-center gap-2 h-7 px-3 rounded-md border",
        "text-sm font-medium text-fg-secondary border-border-default bg-transparent",
        "transition-colors duration-100 ease-out",
        "enabled:hover:bg-bg-hover enabled:hover:text-fg-primary enabled:hover:border-border-strong",
        "enabled:active:bg-bg-selected",
        "disabled:text-fg-disabled disabled:border-border-subtle disabled:cursor-not-allowed",
      )}
    >
      {children}
    </button>
  );
}

function shortenPath(p: string): string {
  if (p.length <= 60) return p;
  const parts = p.split(/[\\/]/);
  if (parts.length <= 3) return p;
  return `…/${parts.slice(-3).join("/")}`;
}
