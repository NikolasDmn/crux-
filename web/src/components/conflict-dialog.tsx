import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/overlays";
import { t } from "@/lib/t";

/** 409 on an edit (frontend-spec 2.2): Reload (take theirs) or Overwrite (resend with their version). */
export function ConflictDialog({
  open,
  onReload,
  onOverwrite,
  onClose,
}: {
  open: boolean;
  onReload: () => void;
  onOverwrite: () => void;
  onClose: () => void;
}) {
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title={t("conflict.title")}
      description={t("conflict.body")}
      footer={
        <>
          <Button onClick={onReload}>{t("conflict.reload")}</Button>
          <Button variant="primary" onClick={onOverwrite}>
            {t("conflict.overwrite")}
          </Button>
        </>
      }
    />
  );
}
