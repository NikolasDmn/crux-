# Old exam board importer (internal)

Brings a course export from the old exam board (the Claude artifact) into Crux, through the API
as a normal signed-in user. Python 3.10+, standard library only.

```bash
# What it would do (reads the export, changes nothing)
python tools/importer/crux_import.py imports/bsa-export --dry-run

# Import into the server at --base as --login (asks for the password, or CRUX_IMPORT_PASSWORD)
python tools/importer/crux_import.py imports/bsa-export --base https://crux.<your domain> --login <you>
```

- **Course:** code and name from the export's README title (`# BSA (Biomedical Signal Analysis) export`);
  `--code`/`--name` override them. If you already have a course with that code it stops, unless
  you pass `--into-existing`.
- **Lectures:** from the README's `Lectures: L0 = …, L1 …` line.
- **Concepts:** one per question tag (`polyfit-least-squares` → "Polyfit least squares").
- **Materials:** slides by lecture number, the manual and the course mapping (L0), textbook
  chapters by the course's lecture-to-chapter mapping (unmapped chapters and appendices go to L0;
  front matter is skipped), every past-exam PDF (L0), and the study-aid notes (`02-study-aids/*.md`).
  PDFs are uploaded and virus-scanned like any upload.
- **Past papers:** from `papers.json`, with key status, left-out questions and the source PDF.
- **Questions:** from `questions.json`: MC options with the key, open model answers, explanations,
  key reliability, source (past exam / imported / lecture), paper and question number, concepts.
  Figures become one-page PDFs and are cropped into the question's image (so a question with a
  figure has two versions: the import and "image cropped").
- **Flashcards:** course cards by lecture. Crux numbers cards itself, so codes can shift where the
  export skipped a number.
- **Left out:** the Obsidian vault and the exercise notebooks and data (not PDFs).

## Other courses

Built on the BSA export, but nothing in it is BSA-specific. When an export differs:

- **No `# CODE (Name) export` title:** the code comes from the folder name (`rki-export` → RKI)
  and the name from the README's first heading; pass `--code`/`--name` if neither works.
- **No `Lectures:` line:** every lecture code the questions, cards and file names use (L0, L3, …)
  becomes a lecture called "Lecture N"; rename them in Crux afterwards.
- **Other folder names:** every PDF under `01-materials` is imported. Its lecture comes from its
  name (`Lecture05`, `L4_slides`, `wk2-L06`), else the first lecture; its kind from its folder
  (slides, exam, exercise/tutorial, book), else Other.
- **No textbook mapping, `papers.json` or `flashcards.json`:** those parts are skipped (the run
  says so).

Always start with `--dry-run`: it prints every lecture and file with where it would go.

Re-runnable: what it created is recorded in `<export>/.crux-import.json` per server, so running it
again only adds what's missing (after a failure, just run it again). Exports belong in `imports/`,
which git ignores.
