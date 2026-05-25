// ReviewPanel — container with sub-tab strip (Queue · Proofread).
//
// Queue is the default sub-tab. Proofread renders the read-only manuscript
// view for final project sign-off.
//
// The print stylesheet is imported here so @media print rules are always in
// scope when ReviewPanel is mounted.

import "./print.css";

import { useState } from "react";
import { cn } from "../../lib/cn";
import type {
  CatalogResponse,
  GateReport,
  ProjectSummary,
  ReviewQueueResponse,
  UnitId,
} from "../../lib/types";
import { ProofreadView } from "./ProofreadView";
import { ReviewQueue } from "./ReviewQueue/ReviewQueue";

type SubTab = "queue" | "proofread";

export interface ReviewPanelProps {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  reports: Record<UnitId, GateReport>;
  reviewQueue: ReviewQueueResponse | null;
  onOpenItem: (catalogPath: string, unitId: UnitId) => void;
  /** Called when the user clicks a unit in Proofread to jump to Translate. */
  onNavigateToUnit: (
    catalogPath: string,
    unitId: UnitId,
    locale?: string,
  ) => void;
  /** Called when "Open remaining hard flags" is clicked in Proofread. */
  onOpenHardFlags: () => void;
}

export function ReviewPanel({
  summary,
  openCatalogs,
  reports,
  reviewQueue,
  onOpenItem,
  onNavigateToUnit,
  onOpenHardFlags,
}: ReviewPanelProps) {
  const [subTab, setSubTab] = useState<SubTab>("queue");

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
      }}
    >
      {/* Sub-tab strip */}
      <div
        className="review-panel-subtabs"
        role="tablist"
        aria-label="Review sub-view"
        style={{
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: 2,
          padding: "0 12px",
          height: 38,
          borderBottom: "1px solid var(--color-border-subtle)",
          background: "var(--color-bg-surface)",
        }}
      >
        <SubTabButton
          active={subTab === "queue"}
          onClick={() => setSubTab("queue")}
          id="subtab-queue"
          controls="subtab-panel-queue"
        >
          Queue
        </SubTabButton>
        <SubTabButton
          active={subTab === "proofread"}
          onClick={() => setSubTab("proofread")}
          id="subtab-proofread"
          controls="subtab-panel-proofread"
        >
          Proofread
        </SubTabButton>
      </div>

      {/* Queue panel */}
      <div
        id="subtab-panel-queue"
        role="tabpanel"
        aria-labelledby="subtab-queue"
        style={{
          flex: 1,
          display: subTab === "queue" ? "flex" : "none",
          overflow: "hidden",
          minHeight: 0,
        }}
      >
        {reviewQueue ? (
          <ReviewQueue reviewQueue={reviewQueue} onOpenItem={onOpenItem} />
        ) : (
          <div
            style={{
              flex: 1,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 13,
              color: "var(--color-fg-tertiary)",
            }}
          >
            Loading review queue&hellip;
          </div>
        )}
      </div>

      {/* Proofread panel */}
      <div
        id="subtab-panel-proofread"
        role="tabpanel"
        aria-labelledby="subtab-proofread"
        style={{
          flex: 1,
          display: subTab === "proofread" ? "flex" : "none",
          overflow: "hidden",
          minHeight: 0,
        }}
      >
        <ProofreadView
          summary={summary}
          openCatalogs={openCatalogs}
          reports={reports}
          onNavigateToUnit={onNavigateToUnit}
          onOpenHardFlags={onOpenHardFlags}
        />
      </div>
    </div>
  );
}

// ── SubTabButton ──────────────────────────────────────────────────────────────

function SubTabButton({
  active,
  onClick,
  children,
  id,
  controls,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  id: string;
  controls: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      id={id}
      aria-selected={active}
      aria-controls={controls}
      onClick={onClick}
      className={cn(
        "inline-flex items-center h-6 px-2.5 rounded-sm text-xs font-medium",
        "transition-colors duration-100 ease-out",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        active
          ? "text-fg-primary bg-accent-subtle border border-accent-subtle-border"
          : "text-fg-tertiary border border-transparent hover:text-fg-primary hover:bg-bg-hover",
      )}
    >
      {children}
    </button>
  );
}
