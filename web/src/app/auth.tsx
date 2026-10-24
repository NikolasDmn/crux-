import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { restore, type User } from "@/features/auth/api";

export interface Auth {
  user: User | null;
  /** After login/signup, or when /me changed. */
  setUser: (u: User | null) => void;
}

const Ctx = createContext<Auth>({ user: null, setUser: () => {} });

export function useAuth(): Auth {
  return useContext(Ctx);
}

/** The signed-in user, or `null` (the guard sends those to /login). */
export function useUser(): User {
  const { user } = useAuth();
  if (!user) throw new Error("useUser outside a signed-in route");
  return user;
}

/** Restores the session from the refresh cookie before the app renders (spec 2.1). */
export function AuthProvider({ children, splash }: { children: (auth: Auth) => ReactNode; splash: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [user, setUserState] = useState<User | null>(null);
  useEffect(() => {
    let live = true;
    void restore().then((u) => {
      if (!live) return;
      setUserState(u);
      setReady(true);
    });
    return () => {
      live = false;
    };
  }, []);
  const setUser = useCallback((u: User | null) => setUserState(u), []);
  const auth = useMemo(() => ({ user, setUser }), [user, setUser]);
  return <Ctx.Provider value={auth}>{ready ? children(auth) : splash}</Ctx.Provider>;
}
