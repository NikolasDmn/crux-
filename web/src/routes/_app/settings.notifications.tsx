import { createFileRoute } from "@tanstack/react-router";
import { Lock } from "lucide-react";
import { Banner } from "@/components/ui/banner";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/misc";
import { useToast } from "@/components/ui/toast";
import { usePreferences, useSetPreferences, type KindPreference, type Preferences } from "@/features/notify/api";
import { messageFor } from "@/lib/errors";

// Notifications (frontend-spec 4): kinds × channels (In-app, Email), all off by default. Kinds that
// need an answer always show in the inbox (locked on). Email needs a verified address and stops
// after a bounce or unsubscribe. Turning a kind on doesn't bring back past items.
export const Route = createFileRoute("/_app/settings/notifications")({ component: Notifications });

type Channel = "InApp" | "Email";

function Toggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      aria-label={label}
      className="size-4 accent-[var(--text)] disabled:opacity-50"
      checked={checked}
      disabled={disabled}
      onChange={(e) => onChange(e.currentTarget.checked)}
    />
  );
}

function Row({ k, prefs, busy, set }: { k: KindPreference; prefs: Preferences; busy: boolean; set: (kind: string, channel: Channel, on: boolean) => void }) {
  const emailOk = prefs.email.verified && !prefs.email.suppressed;
  return (
    <tr className="border-t border-border">
      <td className="py-3 pr-4">
        <div className="font-medium">{k.label}</div>
        <div className="text-sm text-muted">{k.description}</div>
      </td>
      <td className="w-24 py-3 text-center">
        {k.inAppAlways ? (
          <span className="inline-flex items-center gap-1 text-sm text-muted" title="Always on: it needs an answer">
            <Lock className="size-3.5" aria-hidden />
            Always
          </span>
        ) : (
          <Toggle label={`${k.label} in the inbox`} checked={k.inApp} disabled={busy} onChange={(v) => set(k.kind, "InApp", v)} />
        )}
      </td>
      <td className="w-24 py-3 text-center">
        <Toggle
          label={`${k.label} by email`}
          checked={k.email && emailOk}
          disabled={busy || !emailOk}
          onChange={(v) => set(k.kind, "Email", v)}
        />
      </td>
    </tr>
  );
}

function Notifications() {
  const prefs = usePreferences();
  const save = useSetPreferences();
  const toast = useToast();
  if (prefs.isPending) return <Skeleton className="h-96" />;
  if (prefs.isError) return <Banner tone="danger">{messageFor(prefs.error)}</Banner>;
  const p = prefs.data;
  const set = (kind: string, channel: Channel, enabled: boolean) =>
    save.mutate([{ kind, channel, enabled }], { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) });

  return (
    <>
      <p className="m-0 text-muted">
        Everything is off until you turn it on. Turning one on starts from now; it doesn't bring back earlier ones.
      </p>
      {p.email.suppressed ? (
        <Banner tone="warning">
          Emails to {p.email.address ?? "your address"} have stopped (it bounced, or you used an unsubscribe link).
          Inbox notifications still arrive.
        </Banner>
      ) : !p.email.verified ? (
        <Banner>Verify your email to get notifications by email.</Banner>
      ) : null}
      <Card className="overflow-x-auto py-2">
        <table className="w-full border-collapse text-left">
          <thead>
            <tr className="text-sm text-muted">
              <th className="py-2 pr-4 font-medium">Notification</th>
              <th className="w-24 py-2 text-center font-medium">Inbox</th>
              <th className="w-24 py-2 text-center font-medium">Email</th>
            </tr>
          </thead>
          <tbody>
            {p.kinds.map((k) => (
              <Row key={k.kind} k={k} prefs={p} busy={save.isPending} set={set} />
            ))}
          </tbody>
        </table>
      </Card>
    </>
  );
}
