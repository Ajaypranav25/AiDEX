import { useState } from "react";
import { EmptyState, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { FileCodeIcon, SearchIcon } from "@aidex/ui/icons";
import { Badge, SearchField, Stack } from "@aidex/ui/primitives";
import { retrieve } from "./retrieval";
import type { ProjectProfile } from "./sampleProjects";

type Props = { profile: ProjectProfile };

/** Shows which indexed chunks a question would pull into context. */
export function RetrievalTest({ profile }: Props) {
  const [query, setQuery] = useState(profile.sampleQuery);
  const results = retrieve(profile, query);

  return (
    <section aria-labelledby="memory-retrieval-title">
      <Stack gap={3}>
        <SectionHeader
          title={<span id="memory-retrieval-title">Retrieval test</span>}
          meta={query.trim() ? `${results.length} ${results.length === 1 ? "match" : "matches"}` : undefined}
        />
        <SearchField
          label="Test a question against the index"
          placeholder="Ask what an agent would look up, e.g. where sessions are refreshed"
          size="md"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        {results.length > 0 ? (
          <List aria-live="polite" metaWidth="104px">
            {results.map((result) => (
              <ListRow
                key={result.id}
                media={<FileCodeIcon />}
                title={result.path}
                subtitle={result.preview}
                meta={[
                  `Lines ${result.startLine}–${result.endLine}`,
                  <Badge key="score" tone="neutral">
                    Score {result.score.toFixed(2)}
                  </Badge>,
                ]}
              />
            ))}
          </List>
        ) : (
          <EmptyState
            icon={<SearchIcon size={28} />}
            title={query.trim() ? "No matching chunks" : "Type a question to test retrieval"}
            description={
              query.trim()
                ? "Try words that appear in the code, such as a function or file name."
                : "Results show the files and lines an agent would receive as context."
            }
          />
        )}
      </Stack>
    </section>
  );
}
