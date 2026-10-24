import { api, refresh, unwrap, type Schemas } from "@/lib/api";
import { setToken } from "@/lib/token";

export type User = Schemas["UserView"];
export type Session = Schemas["SessionResponse"];

/** Signs in and keeps the access token in memory. */
export async function login(body: Schemas["LoginInput"]): Promise<User> {
  const s = await unwrap(api.POST("/auth/login", { body }));
  setToken(s.accessToken);
  return s.user;
}

export async function signup(body: Schemas["SignupInput"]): Promise<User> {
  const s = await unwrap(api.POST("/auth/signup", { body }));
  setToken(s.accessToken);
  return s.user;
}

export async function logout(): Promise<void> {
  try {
    await api.POST("/auth/logout");
  } finally {
    setToken(null);
  }
}

export function forgot(email: string) {
  return unwrap(api.POST("/auth/password/forgot", { body: { email } }));
}

export function resetPassword(body: Schemas["ResetInput"]) {
  return unwrap(api.POST("/auth/password/reset", { body }));
}

export function confirmEmail(token: string) {
  return unwrap(api.POST("/auth/verify-email/confirm", { body: { token } }));
}

export function resendVerification() {
  return unwrap(api.POST("/auth/verify-email"));
}

export function me(): Promise<User> {
  return unwrap(api.GET("/me"));
}

/** At start-up: a session from the refresh cookie, if there is one. */
export async function restore(): Promise<User | null> {
  if (!(await refresh())) return null;
  try {
    return await me();
  } catch {
    setToken(null);
    return null;
  }
}

export function deletionStatus() {
  return unwrap(api.GET("/account/deletion"));
}

export function cancelDeletion() {
  return unwrap(api.POST("/account/delete/cancel"));
}
