import type { Unit } from "../../lib/types";

interface Props {
  unit: Unit;
}

export function Inspector({ unit }: Props) {
  const isPlural = unit.plural_arity != null;
  const placeholders = unit.placeholders ?? [];
  const placeholderCount = Array.isArray(placeholders) ? placeholders.length : 0;
  const provenance = unit.provenance;

  return (
    <aside className="app-chrome shrink-0 w-80 min-w-[240px] flex flex-col overflow-hidden bg-bg-surface border-l border-border-subtle">
      <header className="px-4 py-3 border-b border-border-subtle">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          Inspector
        </span>
      </header>

      <dl className="m-0 px-4 py-3 flex flex-col gap-3 border-b border-border-subtle">
        <Item label="State">
          <CodeText>{unit.state}</CodeText>
        </Item>

        <Item label="Plural arity">
          {isPlural ? (
            <CodeText>
              {unit.plural_arity}&nbsp;forms
            </CodeText>
          ) : (
            <Muted>singular</Muted>
          )}
        </Item>

        <Item label="Placeholders">
          {placeholderCount > 0 ? (
            <CodeText>{placeholderCount}</CodeText>
          ) : (
            <Muted>none</Muted>
          )}
        </Item>

        <Item label="Source location">
          {provenance.file ? (
            <CodeText>
              {provenance.file}
              {provenance.line ? `:${provenance.line}` : ""}
            </CodeText>
          ) : (
            <Muted>—</Muted>
          )}
        </Item>
      </dl>

      <div className="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-2">
        <div className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          Findings
        </div>
        <div className="text-sm text-fg-secondary leading-[1.65]">
          <Muted>
            No findings. The gate runs when this unit is translated; later
            milestones surface its hard and soft flags here.
          </Muted>
        </div>
      </div>
    </aside>
  );
}

function Item({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[110px_1fr] gap-3 items-baseline">
      <dt className="m-0 text-xs uppercase tracking-loose text-fg-tertiary">
        {label}
      </dt>
      <dd className="m-0 text-sm text-fg-secondary [overflow-wrap:anywhere]">
        {children}
      </dd>
    </div>
  );
}

function CodeText({ children }: { children: React.ReactNode }) {
  return (
    <span className="font-mono text-sm text-fg-primary">{children}</span>
  );
}

function Muted({ children }: { children: React.ReactNode }) {
  return <span className="text-fg-tertiary">{children}</span>;
}
