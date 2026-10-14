export type MobilePlatform = 'ios' | 'android';
export type ReleaseStage = 'draft' | 'candidate' | 'in_review' | 'approved' | 'rejected' | 'submitted' | 'rolling_out' | 'released' | 'halted' | 'superseded';
export type AutomatedDecision = 'go' | 'at_risk' | 'no_go';

export interface MobileRelease {
  schema_version: '1.0';
  id: string;
  version: string;
  name: string | null;
  owner: string | null;
  target_date?: string | null;
  submission_date?: string | null;
  rollout_date?: string | null;
  stage: ReleaseStage;
  created_at: string;
  updated_at: string;
  candidate_ids: string[];
  active_candidates: Partial<Record<MobilePlatform, string>>;
  audit_ids: string[];
  active_audits: Partial<Record<MobilePlatform, string>>;
  evidence_link_ids: string[];
  resolution_ids: string[];
  decision_ids: string[];
  active_decisions: Partial<Record<MobilePlatform, string>>;
  waiver_ids: string[];
  active_waiver_ids: string[];
  latest_event_sequence: number;
  latest_event_hash: string | null;
}

export interface ReleasePortfolio {
  schema_version: '1.0';
  generated_at: string;
  counts: Record<string, number>;
  releases: Array<Record<string, unknown>>;
  review_queue: Array<Record<string, unknown>>;
}

export interface ReleaseView {
  release: MobileRelease;
  candidates: Array<Record<string, unknown>>;
  audits: Array<Record<string, unknown>>;
  evidence_links: Array<Record<string, unknown>>;
  resolutions: Array<Record<string, unknown>>;
  decisions: Array<Record<string, unknown>>;
  waivers: Array<Record<string, unknown>>;
  events: Array<Record<string, unknown>>;
}

export function validateMobileRelease(value: unknown): MobileRelease;
export function validateReleaseList(value: unknown): MobileRelease[];
export function validateReleasePortfolio(value: unknown): ReleasePortfolio;
export function validateReleaseView(value: unknown): ReleaseView;
export function validateMobileAuditResult(value: unknown): { report: Record<string, unknown> };
export function validateReleaseAuditResult(value: unknown): Record<string, unknown>;
export function validateSchemaRecord(value: unknown, contract: string, version?: string): Record<string, unknown>;
