export const FEEDBACK_SCHEMA_VERSION = 1 as const;
export const FEEDBACK_LIMITS = {
  contentMinLength: 10,
  contentMaxLength: 5000,
  emailMaxLength: 254,
  attachmentCount: 5,
  attachmentBytes: 10 * 1024 * 1024,
} as const;

export const FEEDBACK_CATEGORY_IDS = {
  issue: 'issue',
  suggestion: 'suggestion',
  other: 'other',
} as const;

export type FeedbackCategory = (typeof FEEDBACK_CATEGORY_IDS)[keyof typeof FEEDBACK_CATEGORY_IDS];

export interface StagedFeedbackAttachment {
  token: string;
  displayName: string;
  mimeType: string;
  size: number;
}

export interface FeedbackSubmissionRequest {
  requestId: string;
  category: FeedbackCategory;
  content: string;
  email: string | null;
  locale: string;
  includeLogs: boolean;
  attachmentTokens: string[];
}

export interface FeedbackSubmissionResult {
  id: string;
  createdAt: string;
  submittedLogCount: number;
}

export const FEEDBACK_ACCEPTED_FILE_TYPES = [
  'image/png',
  'image/jpeg',
  'image/webp',
  'application/pdf',
  'application/zip',
  'text/plain',
] as const;

export type FeedbackAcceptedFileType = (typeof FEEDBACK_ACCEPTED_FILE_TYPES)[number];
