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
  /** Apply a candidate's text into the unit via the normal edit path. */
  onUseConflictCandidate?: (
    catalogPath: string,
    unitId: UnitId,
    forms: string[],
  ) => void;
  onOpenItem: (catalogPath: string, unitId: UnitId) => void;
  /** Called when the user clicks a unit in Proofread to jump to Translate. */
  onNavigateToUnit: (
    catalogPath: string,
    unitId: UnitId,
    locale?: string,
  ) => void;
  /** Called when "Open remaining hard flags" is clicked in Proofread. */
  onOpenHardFlags: () => void;
  /** Forwarded to ProofreadView so it can fan-load every project catalog
   *  on first render — without this the manuscript stays empty until the
   *  user manually opens each catalog from Translate. */
  onEnsureCatalogLoaded?: (absPath: string) => Promise<void>;
  /** Forwarded to ProofreadView for Copy/Save As feedback. */
  onToast?: (message: string, kind: "info" | "error") => void;
}

export function ReviewPanel({
  summary,
  openCatalogs,
  reports,
  reviewQueue,
  onUseConflictCandidate,
  onOpenItem,
  onNavigateToUnit,
  onOpenHardFlags,
  onEnsureCatalogLoaded,
  onToast,
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
      {/* Sub-tab strip — primary navigation between Queue and Proofread.
          Sits on bg-base (not bg-surface) so it reads as a distinct
          navigation row above the panel contents. */}
      <div
        className="review-panel-subtabs"
        role="tablist"
        aria-label="Review sub-view"
        style={{
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: 4,
          padding: "8px 16px 0",
          height: 48,
          borderBottom: "1px solid var(--color-border-default)",
          background: "var(--color-bg-base)",
        }}
      >
        <SubTabButton
          active={subTab === "queue"}
          onClick={() => setSubTab("queue")}
          id="subtab-queue"
          controls="subtab-panel-queue"
          icon={<FolderTabIcon />}
        >
          Queue
        </SubTabButton>
        <SubTabButton
          active={subTab === "proofread"}
          onClick={() => setSubTab("proofread")}
          id="subtab-proofread"
          controls="subtab-panel-proofread"
          icon={<BookTabIcon />}
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
          <ReviewQueue
            reviewQueue={reviewQueue}
            onUseConflictCandidate={onUseConflictCandidate}
            onOpenItem={onOpenItem}
          />
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
          onEnsureCatalogLoaded={onEnsureCatalogLoaded}
          onToast={onToast}
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
  icon,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  id: string;
  controls: string;
  icon?: React.ReactNode;
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
        "relative inline-flex items-center gap-2 h-10 px-3 text-sm font-medium",
        "transition-colors duration-100 ease-out",
        "border-b-2",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        active
          ? "text-fg-primary border-accent"
          : "text-fg-tertiary border-transparent hover:text-fg-secondary hover:border-border-default",
      )}
    >
      {icon && (
        <span
          aria-hidden="true"
          className={active ? "text-accent" : "text-fg-tertiary"}
          style={{ display: "inline-flex" }}
        >
          {icon}
        </span>
      )}
      {children}
    </button>
  );
}

// ── Inline tab icons ──────────────────────────────────────────────────────────

function FolderTabIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    </svg>
  );
}

function BookTabIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
      <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
    </svg>
  );
}
