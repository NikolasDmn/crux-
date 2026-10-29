import { unlock } from "./support/stack";

// Signups and logins are rate limited per IP (5 signups an hour), and every journey signs up fresh
// users, so the limits are cleared first (and again by signedUp() when a run hits them).
export default function globalSetup(): void {
  try {
    unlock();
  } catch {
    console.warn("e2e: clearing the rate limits failed; signups may hit the hourly limit");
  }
}
