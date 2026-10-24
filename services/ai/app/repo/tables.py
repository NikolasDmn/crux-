"""SQLAlchemy Core tables over schema-v5-ai.sql (columns the service reads or writes)."""

from sqlalchemy import (
    JSON,
    BigInteger,
    Boolean,
    Column,
    DateTime,
    Integer,
    MetaData,
    Numeric,
    SmallInteger,
    String,
    Table,
    Text,
)

metadata = MetaData()

settings = Table(
    "UserAiSettings",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("Enabled", Boolean, nullable=False),
    Column("AllowMaterialChat", Boolean, nullable=False),
    Column("AllowGrading", Boolean, nullable=False),
    Column("AllowCourseImport", Boolean, nullable=False),
    Column("AllowPastExamImport", Boolean, nullable=False),
    Column("AllowGeneration", Boolean, nullable=False),
    Column("NightlyGeneration", Boolean, nullable=False),
    Column("ShareProgress", Boolean, nullable=False),
    Column("ShareMistakes", Boolean, nullable=False),
    Column("ShareAnswers", Boolean, nullable=False),
    Column("AutoApproveGenerated", Boolean, nullable=False),
    Column("DefaultProviderKeyId", BigInteger),
    Column("DefaultModel", String(64)),
    Column("DefaultTutorTemplateId", BigInteger),
    Column("MonthlyCapMicroUsd", BigInteger),
    Column("ConsentPolicyVersion", String(16)),
    Column("ConsentedAt", DateTime),
    Column("UpdatedAt", DateTime),
    Column("RowVersion", Integer, nullable=False),
)

provider_key = Table(
    "AiProviderKey",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("Provider", String(32), nullable=False),
    Column("Label", String(64), nullable=False),
    Column("Ciphertext", String(1024), nullable=False),  # base64 of the sealed blob (VARBINARY column)
    Column("KmsKeyId", String(64), nullable=False),
    Column("Last4", String(4), nullable=False),
    Column("Status", String(16), nullable=False),
    Column("LastVerifiedAt", DateTime),
    Column("CreatedAt", DateTime),
    Column("RevokedAt", DateTime),
)

tutor = Table(
    "AiTutorTemplate",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("TemplateKey", String(48), nullable=False),
    Column("Version", Integer, nullable=False),
    Column("OwnerUserRef", String(26)),
    Column("Name", String(64), nullable=False),
    Column("Description", String(255), nullable=False),
    Column("Personality", String(16), nullable=False),
    Column("TeachingStyle", String(16), nullable=False),
    Column("Rules", JSON, nullable=False),
    Column("PromptText", Text, nullable=False),
    Column("IsActive", Boolean, nullable=False),
    Column("CreatedAt", DateTime),
)

course_setting = Table(
    "AiCourseSetting",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("CourseRef", String(26), primary_key=True),
    Column("AiEnabled", Boolean),
    Column("TutorTemplateId", BigInteger),
    Column("NightlyGeneration", Boolean),
    Column("GenerationPolicy", JSON),
)

consent_log = Table(
    "AiConsentLog",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("UserRef", String(26), nullable=False),
    Column("ChangedAt", DateTime, nullable=False),
    Column("PolicyVersion", String(16), nullable=False),
    Column("Delta", JSON, nullable=False),
)

run = Table(
    "AiRun",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("CourseRef", String(26)),
    Column("ParentRunId", BigInteger),
    Column("Feature", String(16), nullable=False),
    Column("ProviderKeyId", BigInteger),
    Column("Provider", String(32), nullable=False),
    Column("Model", String(64), nullable=False),
    Column("PromptVersion", String(32), nullable=False),
    Column("TutorTemplateId", BigInteger),
    Column("IdempotencyKey", String(64)),
    Column("Status", String(16), nullable=False),
    Column("InputTokens", Integer),
    Column("OutputTokens", Integer),
    Column("CostMicroUsd", BigInteger),
    Column("TranscriptRef", String(512)),
    Column("RetainUntil", DateTime),
    Column("Error", Text),
    Column("CreatedAt", DateTime),
    Column("StartedAt", DateTime),
    Column("FinishedAt", DateTime),
)

usage_monthly = Table(
    "AiUsageMonthly",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("Month", String(7), primary_key=True),
    Column("SpentMicroUsd", BigInteger, nullable=False),
    Column("ReservedMicroUsd", BigInteger, nullable=False),
    Column("Runs", Integer, nullable=False),
)

run_context = Table(
    "AiRunContext",
    metadata,
    Column("AiRunId", BigInteger, primary_key=True),
    Column("EntityType", String(32), primary_key=True),
    Column("EntityRef", String(40), primary_key=True),
)

conversation = Table(
    "AiConversation",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("CourseRef", String(26), nullable=False),
    Column("Scope", String(16), nullable=False),
    Column("MaterialRef", String(26)),
    Column("ComponentRef", String(26)),
    Column("QuestionRef", String(26)),
    Column("MockRef", String(26)),
    Column("TutorTemplateId", BigInteger),
    Column("Title", String(255)),
    Column("CreatedAt", DateTime),
    Column("ArchivedAt", DateTime),
)

message = Table(
    "AiMessage",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("ConversationId", BigInteger, nullable=False),
    Column("Role", String(16), nullable=False),
    Column("Content", Text, nullable=False),
    Column("Anchor", JSON),
    Column("AiRunId", BigInteger),
    Column("Feedback", String(8)),
    Column("ServedByModel", String(64)),
    Column("CreatedAt", DateTime),
)

citation = Table(
    "AiCitation",
    metadata,
    Column("MessageId", BigInteger, primary_key=True),
    Column("FileRef", String(26), primary_key=True),
    Column("ChunkNo", Integer, primary_key=True),
    Column("PageNo", Integer),
    Column("Quote", String(500)),
)

grade = Table(
    "AiGrade",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("MockRef", String(26), nullable=False),
    Column("QuestionOrder", Integer, nullable=False),
    Column("QuestionVersionRef", String(26)),
    Column("AiRunId", BigInteger, nullable=False),
    Column("SuggestedOutcome", String(8), nullable=False),
    Column("SuggestedGrade", Numeric(6, 2), nullable=False),
    Column("MaxGrade", Numeric(6, 2), nullable=False),
    Column("Feedback", Text, nullable=False),
    Column("SlipsJson", JSON),
    Column("Status", String(16), nullable=False),
    Column("DecidedAt", DateTime),
    Column("CreatedAt", DateTime),
)

course_import = Table(
    "CourseImport",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("CourseRef", String(26)),
    Column("Status", String(16), nullable=False),
    Column("Instructions", Text),
    Column("ImportPastExams", Boolean, nullable=False),
    Column("AiRunId", BigInteger),
    Column("Parts", SmallInteger),
    Column("Truncated", Boolean, nullable=False),
    Column("CreatedAt", DateTime),
    Column("AppliedAt", DateTime),
    Column("RowVersion", Integer, nullable=False),
)

import_file = Table(
    "CourseImportFile",
    metadata,
    Column("ImportId", BigInteger, primary_key=True),
    Column("FileRef", String(26), primary_key=True),
    Column("DetectedKind", String(16)),
    Column("Status", String(16), nullable=False),
    Column("Note", String(255)),
)

proposal = Table(
    "ImportProposal",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("ImportId", BigInteger, nullable=False),
    Column("Kind", String(20), nullable=False),
    Column("TempKey", String(64), nullable=False),
    Column("ParentTempKey", String(64)),
    Column("Payload", JSON, nullable=False),
    Column("PayloadSchemaVersion", SmallInteger, nullable=False),
    Column("UserEdits", JSON),
    Column("SourceFileRef", String(26)),
    Column("SourcePage", Integer),
    Column("Confidence", String(8), nullable=False),
    Column("Rationale", String(500)),
    Column("DuplicateOfRef", String(26)),
    Column("Status", String(16), nullable=False),
    Column("CreatedEntityRef", String(26)),
    Column("RowVersion", Integer, nullable=False),
)

generation_job = Table(
    "GenerationJob",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("CourseRef", String(26), nullable=False),
    Column("TriggeredBy", String(8), nullable=False),
    Column("Request", JSON, nullable=False),
    Column("Status", String(16), nullable=False),
    Column("AiRunId", BigInteger),
    Column("CreatedCount", Integer, nullable=False),
    Column("RejectedCount", Integer, nullable=False),
    Column("CreatedAt", DateTime),
    Column("FinishedAt", DateTime),
)
