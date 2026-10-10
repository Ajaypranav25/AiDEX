import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { FileCodeIcon, HistoryIcon, PlayIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { useToast } from "../../app/providers/toast";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { changedFiles, fileById, RUN_TESTS_MESSAGE } from "./sample";

export function ReviewSidebar() {
  const { route, navigate, replace } = useNavigation();
  const notify = useToast();
  const selected = fileById(route.id);

  return (
    <SectionSidebar title="Review">
      {/* Counts are changed lines (additions plus deletions) per file. */}
      <SidebarSection title="Changed files">
        {changedFiles.map((file) => (
          <SidebarItem
            key={file.id}
            label={file.path}
            icon={<FileCodeIcon />}
            count={file.additions + file.deletions}
            active={file.id === selected.id}
            onSelect={() => replace({ ...route, id: file.id })}
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Tools">
        <SidebarItem label="Run tests" icon={<PlayIcon />} onSelect={() => notify(RUN_TESTS_MESSAGE)} />
        <SidebarItem label="Checkpoints" icon={<HistoryIcon />} onSelect={() => navigate({ page: "checkpoints" })} />
      </SidebarSection>
    </SectionSidebar>
  );
}
