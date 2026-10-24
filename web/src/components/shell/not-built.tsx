import { Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import { EmptyList } from "@/components/ui/empty";

/** Placeholder for screens of later build steps (frontend-spec 10), so the shell's links resolve. */
export function NotBuilt({ title, step }: { title: string; step: number }) {
  return (
    <div className="flex flex-col gap-2">
      <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">{title}</h1>
      <EmptyList
        title="Not built yet"
        body={`This screen arrives with build step ${step}.`}
        action={
          <Button asChild size="sm">
            <Link to="/">Back to Home</Link>
          </Button>
        }
      />
    </div>
  );
}
