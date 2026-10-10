import type { ReactNode } from "react";
import { LoadingState, Notice } from "@aidex/ui/components";
import { RefreshIcon } from "@aidex/ui/icons";
import { Button } from "@aidex/ui/primitives";
import type { BridgeQuery } from "../../hooks/useBridgeQuery";

type ErrorProps = {
  query: BridgeQuery<unknown>;
  /** Leads the error message, e.g. "Cannot list workspaces". */
  failure: string;
};

/** Why a query failed, with a retry. Renders nothing while it has not failed. */
export function QueryError({ query, failure }: ErrorProps) {
  if (!query.error) return null;
  return (
    <Notice
      tone="warning"
      action={
        <Button variant="outline" size="md" icon={<RefreshIcon />} onClick={query.refetch}>
          Try again
        </Button>
      }
    >
      {failure}: {query.error.message}
    </Notice>
  );
}

type StateProps<T> = {
  query: BridgeQuery<T>;
  /** Shown until the first answer arrives, e.g. "Loading workspaces…". */
  loading: string;
  /** Leads the error message, e.g. "Cannot list workspaces". */
  failure: string;
  /** Renders the data once it has arrived. */
  children: (data: T) => ReactNode;
};

/** A query's loading and failure states, with a retry, around what its data renders. */
export function QueryState<T>({ query, loading, failure, children }: StateProps<T>) {
  if (query.error) return <QueryError query={query} failure={failure} />;
  if (query.data === undefined) return <LoadingState label={loading} />;
  return <>{children(query.data)}</>;
}
