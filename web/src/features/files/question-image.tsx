import { useQuery } from "@tanstack/react-query";
import { Skeleton } from "@/components/ui/misc";
import { fileUrl } from "@/features/files/api";
import { sameOrigin } from "@/features/files/lib/upload";
import { cn } from "@/lib/cn";

/** A question's figure (a crop stored by the files service), fetched through a short-lived signed
 *  URL on this origin (CSP img-src 'self'). */
export function QuestionImage({ fileId, className }: { fileId: string; className?: string }) {
  const url = useQuery({
    queryKey: ["file", fileId, "url"],
    queryFn: () => fileUrl(fileId),
    staleTime: 5 * 60_000,
  });
  if (url.isError) return <p className="m-0 text-sm text-muted">The figure for this question couldn't load.</p>;
  if (!url.data) return <Skeleton className={cn("h-40", className)} />;
  return (
    <img
      src={sameOrigin(url.data.url, window.location.origin)}
      alt="Figure for this question"
      className={cn(
        "max-h-[60vh] max-w-full self-start rounded-md border border-border bg-white object-contain",
        className,
      )}
    />
  );
}
