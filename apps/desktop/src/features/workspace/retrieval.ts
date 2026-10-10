// Local stand-in for retrieval: scores sample file chunks by term overlap so the test query
// behaves plausibly until AiDEX Core runs real embedding search.
import type { ProjectProfile } from "./sampleProjects";

export type RetrievalResult = {
  id: string;
  path: string;
  startLine: number;
  endLine: number;
  preview: string;
  score: number;
};

const RESULT_LIMIT = 5;
const BASE_SCORE = 0.55;
const COVERAGE_WEIGHT = 0.35;
const FREQUENCY_WEIGHT = 0.1;
const FREQUENCY_CAP = 5;

type Chunk = Omit<RetrievalResult, "score"> & { text: string };

function chunksOf(profile: ProjectProfile): Chunk[] {
  return Object.entries(profile.snippets).flatMap(([path, { code }]) => {
    let line = 1;
    return code.split(/\n\s*\n/).map((block, index) => {
      const lineCount = block.split("\n").length;
      const chunk = {
        id: `${path}#${index}`,
        path,
        startLine: line,
        endLine: line + lineCount - 1,
        preview: block.split("\n").find((text) => text.trim())?.trim() ?? "",
        text: `${path}\n${block}`.toLowerCase(),
      };
      line += lineCount + 1;
      return chunk;
    });
  });
}

const occurrences = (text: string, term: string): number => text.split(term).length - 1;

/** Best-matching chunks for a query, highest score first. */
export function retrieve(profile: ProjectProfile, query: string): RetrievalResult[] {
  const terms = query.toLowerCase().split(/[^a-z0-9_]+/).filter((term) => term.length > 1);
  if (terms.length === 0) return [];

  return chunksOf(profile)
    .map(({ text, ...chunk }) => {
      const matched = terms.filter((term) => text.includes(term)).length;
      const frequency = Math.min(
        terms.reduce((total, term) => total + occurrences(text, term), 0),
        FREQUENCY_CAP,
      );
      const score =
        matched === 0
          ? 0
          : BASE_SCORE + COVERAGE_WEIGHT * (matched / terms.length) + FREQUENCY_WEIGHT * (frequency / FREQUENCY_CAP);
      return { ...chunk, score };
    })
    .filter((result) => result.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, RESULT_LIMIT);
}
