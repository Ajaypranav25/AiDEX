import type { ReactNode } from "react";
import { AsyncContent } from "@aidex/ui/components";
import type { BridgeQuery } from "../../hooks/useBridgeQuery";

type Props<T> = {
  query: BridgeQuery<T>;
  /** Shown until the first answer arrives, e.g. "Loading workspaces…". */
  loading: string;
  /** Leads the error message and names the retry, e.g. "Cannot list workspaces". */
  failure: string;
  /** Renders the data once it has arrived. It runs during this component's render, so it calls no hooks. */
  children: (data: T) => ReactNode;
};

/**
 * A query's loading and failure states around what its data renders. A failure is announced and
 * offers a retry that keeps keyboard focus in place. Renders nothing while the query is disabled.
 */
export function QueryState<T>({ query, loading, failure, children }: Props<T>) {
  const { data, error } = query;
  if (!error && data === undefined && !query.loading) return null;
  return (
    <AsyncContent
      state={error ? "failed" : data === undefined ? "loading" : "ready"}
      loadingLabel={loading}
      failure={error ? `${failure}: ${error.message}` : undefined}
      onRetry={query.refetch}
      retryLabel={`Try again: ${failure}`}
    >
      {!error && data !== undefined ? children(data) : null}
    </AsyncContent>
  );
}
