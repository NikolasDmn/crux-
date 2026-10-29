-- "Choose all that apply" (an MC question with MultiSelect): several options may be correct, and an
-- answer may pick several. Single-choice answers keep ChosenOptionId; multi-select ones fill
-- ChosenOptionIds (a JSON array of MCOption ids).
ALTER TABLE MCQuestion ADD COLUMN MultiSelect BOOLEAN NOT NULL DEFAULT FALSE AFTER ShuffleOptions;
ALTER TABLE MockQuestion ADD COLUMN ChosenOptionIds JSON NULL AFTER ChosenOptionId;
ALTER TABLE QuestionAttempt ADD COLUMN ChosenOptionIds JSON NULL AFTER ChosenOptionId;
