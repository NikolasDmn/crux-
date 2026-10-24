// Flashcard ratings to POST /flash/reviews with the bearer token; keepalive on page hide, like
// the activity queue (a beacon can't carry the token).

import type { Review } from "@/features/flash/lib/session";
import { BASE } from "./api";
import { getToken } from "./token";

export async function sendReviews(
  reviews: Review[],
  { keepalive }: { keepalive: boolean },
): Promise<{ clientId: string; status: string }[] | null> {
  const token = getToken();
  if (!token) return null;
  const r = await fetch(`${BASE}/flash/reviews`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ reviews }),
    keepalive,
    credentials: "include",
  });
  if (!r.ok) return null;
  const body = (await r.json()) as { results?: { clientId: string; status: string }[] };
  return body.results ?? reviews.map((x) => ({ clientId: x.clientId, status: "applied" }));
}
