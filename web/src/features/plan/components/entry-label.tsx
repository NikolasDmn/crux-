import { entryTitle } from "@/features/plan/lib/week";

/** "BSA Practise 4.1", or just the description when it already starts with the code. */
export function EntryLabel({ entry }: { entry: { courseCode: string; description: string } }) {
  const { code, text } = entryTitle(entry);
  return (
    <>
      {code ? <span className="font-mono text-xs">{code}</span> : null} {text}
    </>
  );
}
