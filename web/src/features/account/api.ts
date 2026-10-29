import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useAuth } from "@/app/auth";
import { api, ifMatch, unwrap, type Schemas } from "@/lib/api";

export type Export = Schemas["ExportView"];
export type Deletion = Schemas["DeletionView"];
export type CourseDecision = Schemas["CourseDecision"];
export type Invite = Schemas["InviteView"];
export type CreatedInvite = Schemas["CreatedInvite"];
export type Settings = Schemas["SettingsView"];

export const keys = {
  exports: ["account", "exports"] as const,
  deletion: ["account", "deletion"] as const,
  invites: ["invites"] as const,
  settings: ["me", "settings"] as const,
};

/** Still being built: the list polls until it settles. */
export function exportBusy(e: Pick<Export, "status">): boolean {
  return e.status === "Queued" || e.status === "Building";
}

/** What the list shows: a Ready export past its 24 h is Expired (the API keeps saying Ready). */
export function exportStatus(e: Pick<Export, "status" | "expiresAt">, now = Date.now()): string {
  if (e.status === "Ready" && e.expiresAt && new Date(e.expiresAt).getTime() <= now) return "Expired";
  return e.status;
}

export function useExports() {
  return useQuery({
    queryKey: keys.exports,
    queryFn: () => unwrap(api.GET("/account/exports")),
    refetchInterval: (q) => (q.state.data?.some(exportBusy) ? 3000 : false),
  });
}

export function useRequestExport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/account/export")),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.exports }),
  });
}

export function useDeletion() {
  return useQuery({ queryKey: keys.deletion, queryFn: () => unwrap(api.GET("/account/deletion")) });
}

/** Schedules or cancels deletion; either way the user's status (and the banner) changes. */
function useDeletionMutation<T>(fn: (input: T) => Promise<Deletion>) {
  const qc = useQueryClient();
  const { setUser } = useAuth();
  return useMutation({
    mutationFn: fn,
    onSuccess: async (view) => {
      qc.setQueryData(keys.deletion, view);
      setUser(await unwrap(api.GET("/me")));
    },
  });
}

export function useScheduleDeletion() {
  return useDeletionMutation((body: { password: string; courses: CourseDecision[] }) =>
    unwrap(api.POST("/account/delete", { body })),
  );
}

export function useCancelDeletion() {
  return useDeletionMutation((_: void) => unwrap(api.POST("/account/delete/cancel")));
}

export function useInvites() {
  return useQuery({
    queryKey: keys.invites,
    queryFn: () => unwrap(api.GET("/invites", { params: { query: { limit: 100 } } })),
  });
}

export function useCreateInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ key, ...body }: Schemas["CreateInviteInput"] & { key: string }) =>
      unwrap(api.POST("/invites", { params: { header: { "Idempotency-Key": key } }, body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.invites }),
  });
}

export function useRevokeInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/invites/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.invites }),
  });
}

/** `PATCH /me`: name, username, time zone. */
export function usePatchMe() {
  const { setUser } = useAuth();
  return useMutation({
    mutationFn: ({ version, ...body }: Schemas["ProfilePatch"] & { version: number }) =>
      unwrap(api.PATCH("/me", { params: { header: ifMatch(version) }, body })),
    onSuccess: (u) => setUser(u),
  });
}

/** Signs out every other device; this one stays signed in. */
export function useChangePassword() {
  return useMutation({
    mutationFn: (body: Schemas["ChangePasswordInput"]) => unwrap(api.POST("/auth/change-password", { body })),
  });
}

/** `PATCH /me/settings`. A planner setting (window, hours, one course a day) rebuilds the plan. */
export function usePatchSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ version, ...body }: Schemas["SettingsPatch"] & { version: number }) =>
      unwrap(api.PATCH("/me/settings", { params: { header: ifMatch(version) }, body })),
    onSuccess: (s) => {
      qc.setQueryData(keys.settings, s);
      void qc.invalidateQueries({ queryKey: ["plan"] });
      void qc.invalidateQueries({ queryKey: ["next"] });
      void qc.invalidateQueries({ queryKey: ["week"] });
      void qc.invalidateQueries({ queryKey: ["flash"] });
    },
  });
}
