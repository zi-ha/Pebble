import { Mail } from "lucide-react";
import { useTranslation } from "react-i18next";

/**
 * Placeholder for the reading pane when no message is selected.
 * The mail layout keeps three columns at all times, so the detail pane never
 * collapses — it shows this instead of disappearing.
 */
export default function MessageDetailEmpty() {
  const { t } = useTranslation();

  return (
    <div className="mail-detail-empty">
      <Mail size={40} strokeWidth={1.1} aria-hidden="true" />
      <p className="mail-detail-empty-title">
        {t("inbox.noSelection", "No message selected")}
      </p>
      <p className="mail-detail-empty-hint">
        {t("inbox.noSelectionHint", "Select a message to read it here")}
      </p>
    </div>
  );
}
