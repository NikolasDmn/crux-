// Notify writes each item's link when it's delivered (services/notify templates). A few of those
// paths name routes the web app spells differently; map them here so old items keep working.

/** The in-app path to open for an inbox item, or `null` when it has none (the item acts inline). */
export function inboxHref(link: string | null | undefined): string | null {
  if (!link) return null;
  const [path = "", query] = link.split("?", 2);
  // Course invitations are answered inline in the inbox and the sidebar.
  if (path === "/courses" || path === "/inbox") return null;
  const mock = /^\/mocks\/([^/]+)\/review$/.exec(path);
  if (mock) return `/mock/${mock[1]}/result`;
  // The check-in form is on the course's Exams page; there's no parameter to pass.
  if (/^\/c\/[^/]+\/exams$/.test(path)) return path;
  return query ? `${path}?${query}` : path;
}
