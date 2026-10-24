import { createFileRoute, notFound } from "@tanstack/react-router";
import { useState } from "react";
import { ThemeMenu } from "@/components/shell/top-bar";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Checklist, DropBox, EmptyList } from "@/components/ui/empty";
import { Field, Input, Select } from "@/components/ui/field";
import { Dot, Kbd, Mark, Skeleton } from "@/components/ui/misc";
import { OptionRow } from "@/components/ui/option-row";
import { Dialog } from "@/components/ui/overlays";
import { PlanBlock } from "@/components/ui/plan-block";
import { ProgressBar } from "@/components/ui/progress";
import { RatingButtons } from "@/components/ui/rating-buttons";
import { Segmented } from "@/components/ui/segmented";
import { SeverityBar } from "@/components/ui/severity";
import { Tab, TabList, TabPanel, Tabs } from "@/components/ui/tabs";
import { useToast } from "@/components/ui/toast";
import { COURSE_COLORS } from "@/lib/color";
import type { Confidence, Rating } from "@/lib/keyboard";

// Dev only: every component skin in one place (frontend-spec 3.3), to check both themes.
export const Route = createFileRoute("/dev/ui")({
  beforeLoad: () => {
    if (!import.meta.env.DEV) throw notFound();
  },
  component: Gallery,
});

function Gallery() {
  const toast = useToast();
  const [picked, setPicked] = useState(1);
  const [conf, setConf] = useState<Confidence>("Sure");
  const [rated, setRated] = useState<Rating | null>(null);
  const [dialog, setDialog] = useState(false);
  const [course, setCourse] = useState(COURSE_COLORS[0]!.hex);

  return (
    <div
      className="mx-auto flex max-w-[1100px] flex-col gap-8 px-4 py-8 md:px-10"
      style={{ ["--course" as string]: course }}
    >
      <header className="flex flex-wrap items-center gap-3">
        <Mark size={32} />
        <h1 className="m-0 flex-1 text-xl font-semibold">Component gallery</h1>
        <div className="flex gap-1.5" role="group" aria-label="Course colour">
          {COURSE_COLORS.map((c) => (
            <button
              key={c.hex}
              type="button"
              aria-label={c.name}
              aria-pressed={course === c.hex}
              onClick={() => setCourse(c.hex)}
              className="size-6 rounded-full border-2 border-transparent aria-pressed:border-text"
              style={{ background: c.hex }}
            />
          ))}
        </div>
        <ThemeMenu />
      </header>

      <section className="grid gap-6 md:grid-cols-2">
        <Card className="flex flex-col gap-4">
          <SectionLabel>Buttons</SectionLabel>
          <div className="flex flex-wrap gap-2">
            <Button variant="primary">Take me there</Button>
            <Button>Secondary</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="destructive">Delete course</Button>
            <Button disabled>Disabled</Button>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button size="sm">Small</Button>
            <Button size="lg" variant="primary">
              Large
            </Button>
            <Button variant="link">A link</Button>
          </div>
        </Card>
        <Card className="flex flex-col gap-4">
          <SectionLabel>Fields</SectionLabel>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Course code">{(ids) => <Input {...ids} defaultValue="BSA" />}</Field>
            <Field label="Study from">
              {(ids) => (
                <Select {...ids} defaultValue="09:00">
                  <option>09:00</option>
                  <option>18:00</option>
                </Select>
              )}
            </Field>
            <Field label="Hours a day" error="Use 0.5–16 hours.">
              {(ids) => <Input {...ids} defaultValue="20" />}
            </Field>
            <Field label="Password" warning="Seen in a data breach.">
              {(ids) => <Input {...ids} type="password" defaultValue="hunter2hunter2" />}
            </Field>
          </div>
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>Badges</SectionLabel>
          <div className="flex flex-wrap gap-2">
            <Badge>Draft</Badge>
            <Badge tone="info">Duplicate</Badge>
            <Badge tone="warning">Disputed</Badge>
            <Badge tone="success">Approved</Badge>
            <Badge tone="danger">Infected</Badge>
            <Badge kind="code">BSA017</Badge>
            <Badge tone="info" kind="label">
              Matches your L3
            </Badge>
          </div>
          <SectionLabel>Progress and severity</SectionLabel>
          <ProgressBar value={7} max={12} label="Lectures 7 / 12" detail="18.5 of 30 h" />
          <SeverityBar topic="CLT" severity="High" />
          <SeverityBar topic="Variance of sums" severity="Medium" />
          <SeverityBar topic="Bayes" severity="Low" />
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>Mock option + confidence</SectionLabel>
          <div role="radiogroup" aria-label="Options" className="flex flex-col gap-2">
            {["σ²", "σ² / n", "σ / √n"].map((o, i) => (
              <OptionRow key={o} index={i} selected={picked === i} onSelect={() => setPicked(i)}>
                {o}
              </OptionRow>
            ))}
            <OptionRow index={3} selected={false} onSelect={() => {}} state="wrong">
              n σ² (graded)
            </OptionRow>
          </div>
          <Segmented
            label="How sure are you?"
            value={conf}
            onChange={setConf}
            options={[
              { value: "Sure", label: "Sure" },
              { value: "Unsure", label: "Unsure" },
              { value: "Guess", label: "Guess" },
            ]}
          />
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>Flashcard rating</SectionLabel>
          <RatingButtons intervals={{ Again: "1 min", Hard: "6 min", Good: "3 d", Easy: "8 d" }} onRate={setRated} />
          <span className="text-sm text-muted">{rated ? `Rated ${rated}` : "Press a rating"}</span>
          <SectionLabel>Plan blocks</SectionLabel>
          <div className="grid grid-cols-2 gap-2">
            <PlanBlock time="14:00–15:30" title="BSA L4 Sampling" />
            <PlanBlock time="19:00–20:30" title="LINALG L5" color="#3e63dd" pinned />
            <PlanBlock time="09:00–10:00" title="BSA revision" status="Done" />
            <PlanBlock time="11:00–12:00" title="BSA L3" status="Skipped" />
          </div>
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>Banners, toast, dialog</SectionLabel>
          <Banner action={<Button variant="link">Resend link</Button>}>
            Verify your email to turn on email notifications and invites.
          </Banner>
          <Banner tone="warning" action={<Button variant="link">Open plan</Button>}>
            <strong>BSA is 6 h short</strong> before its final week.
          </Banner>
          <Banner tone="danger">Your account will be deleted on Sat 7 Nov.</Banner>
          <div className="flex flex-wrap gap-2">
            <Button
              onClick={() =>
                toast({
                  message: "Original removed, 18 MB freed",
                  action: { label: "Undo", run: () => toast({ message: "Restored" }) },
                })
              }
            >
              Show toast
            </Button>
            <Button onClick={() => setDialog(true)}>Open dialog</Button>
          </div>
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>Empty states</SectionLabel>
          <EmptyList
            title="No flashcards yet"
            body="Make one from a slip, or add your own for this lecture."
            action={
              <Button size="sm" variant="primary">
                New flashcard
              </Button>
            }
          />
          <DropBox
            title="Drop lecture slides here"
            hint="PDFs up to 100 MB"
            accept="application/pdf"
            onFiles={(f) => toast({ message: `${f.length} file(s)` })}
          />
          <Checklist
            steps={[
              { label: "Create your course", done: true },
              { label: "Add the exam date", done: false, action: <Button size="sm">Open BSA</Button> },
              { label: "Upload the slides", done: false },
            ]}
          />
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>Tabs, sidebar row, misc</SectionLabel>
          <Tabs defaultValue="m" style={{ ["--tab-accent" as string]: "var(--course)" }}>
            <TabList>
              <Tab value="m">Materials</Tab>
              <Tab value="q">Questions</Tab>
              <Tab value="f">Flashcards</Tab>
            </TabList>
            <TabPanel value="m" className="pt-3 text-muted">
              Materials panel
            </TabPanel>
            <TabPanel value="q" className="pt-3 text-muted">
              Questions panel
            </TabPanel>
            <TabPanel value="f" className="pt-3 text-muted">
              Flashcards panel
            </TabPanel>
          </Tabs>
          <div className="flex h-8 w-[248px] items-center gap-2.5 rounded-[6px] bg-sunken px-2.5 font-medium">
            <Dot />
            <span className="flex-1">BSA</span>
            <span className="font-mono text-xs text-muted">10 d</span>
          </div>
          <div className="flex items-center gap-3">
            <Kbd>⌘K</Kbd>
            <Skeleton className="h-4 w-40" />
          </div>
        </Card>
      </section>

      <Dialog
        open={dialog}
        onOpenChange={setDialog}
        title="Changed somewhere else"
        description="Someone (maybe you, in another tab) changed this since you opened it."
        footer={
          <>
            <Button onClick={() => setDialog(false)}>Reload theirs</Button>
            <Button variant="primary" onClick={() => setDialog(false)}>
              Overwrite with mine
            </Button>
          </>
        }
      />
    </div>
  );
}
