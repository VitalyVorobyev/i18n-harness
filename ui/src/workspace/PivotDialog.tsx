import { Button, Dialog, Field, Select } from "@vitavision/ui";
import { useState } from "react";

import type { CatalogSummary, Language } from "../api";

const SAME = "__same__";

/**
 * "The texts already in this file are German": they become the text to
 * translate from instead of finished translations.
 */
export function PivotDialog(props: {
  catalog: CatalogSummary;
  languages: Language[];
  onClose: () => void;
  onApply: (pivot: string | null) => void;
}) {
  const [value, setValue] = useState(props.catalog.pivot ?? SAME);
  const options = [
    { value: SAME, label: `${props.catalog.language} (already translated)` },
    ...props.languages
      .filter((l) => l.id !== props.catalog.locale)
      .map((l) => ({ value: l.id, label: l.name, note: l.id })),
  ];

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && props.onClose()}
      title="Language of the existing text"
      description={`Sometimes a ${props.catalog.language} file arrives filled with another language — for example a copy of the German file. Tell the app which language the texts in ${props.catalog.name} are really in. They are then translated from that language instead of counting as finished.`}
      footer={
        <>
          <Button variant="ghost" onClick={props.onClose}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => props.onApply(value === SAME ? null : value)}>
            Apply
          </Button>
        </>
      }
    >
      <Field label="The texts in this file are in" as="group">
        <Select
          aria-label="Language of the existing text"
          value={value}
          onValueChange={setValue}
          options={options}
        />
      </Field>
    </Dialog>
  );
}
