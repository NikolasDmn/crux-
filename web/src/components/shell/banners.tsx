import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useAuth, useUser } from "@/app/auth";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/ui/toast";
import { cancelDeletion, deletionStatus, me, resendVerification } from "@/features/auth/api";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";
import { t } from "@/lib/t";

/** The banners every signed-in page shows when they apply (frontend-spec 2.1). */
export function AccountBanners() {
  const user = useUser();
  const { setUser } = useAuth();
  const toast = useToast();
  const [sent, setSent] = useState(false);
  const pending = user.status === "PendingDeletion";
  const deletion = useQuery({ queryKey: ["account", "deletion"], queryFn: deletionStatus, enabled: pending });
  const resend = useMutation({
    mutationFn: resendVerification,
    onSuccess: () => setSent(true),
    onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
  });
  const cancel = useMutation({
    mutationFn: cancelDeletion,
    onSuccess: async () => setUser(await me()),
    onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
  });

  return (
    <>
      {pending ? (
        <Banner
          tone="danger"
          action={
            <Button
              size="sm"
              className="h-[30px] border-danger-text/30 text-danger-text"
              disabled={cancel.isPending}
              onClick={() => cancel.mutate()}
            >
              {t("banner.deletion.cancel")}
            </Button>
          }
        >
          {t("banner.deletion", {
            date: deletion.data?.runsAt ? formatInZone(deletion.data.runsAt, user.timeZone, "EEE d MMM") : "…",
          })}
        </Banner>
      ) : null}
      {!user.emailVerified ? (
        <Banner
          action={
            sent ? (
              <span className="text-sm text-muted">{t("banner.verify.sent")}</span>
            ) : (
              <Button variant="link" disabled={resend.isPending} onClick={() => resend.mutate()}>
                {t("banner.verify.resend")}
              </Button>
            )
          }
        >
          {t("banner.verify")}
        </Banner>
      ) : null}
    </>
  );
}
