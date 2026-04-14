import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

interface Props {
  onSubmit: (url: string) => void;
}

export function YouTubeInput({ onSubmit }: Props) {
  const [url, setUrl] = useState("");

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = url.trim();
    if (!trimmed) return;
    onSubmit(trimmed);
    setUrl("");
  };

  return (
    <form onSubmit={submit} className="flex items-center gap-2">
      <span className="text-lg" aria-hidden="true">
        📺
      </span>
      <FieldGroup className="flex-1 gap-0">
        <Field>
          <Input
            type="url"
            placeholder="YouTube URL…"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            className="h-8 bg-background"
          />
        </Field>
      </FieldGroup>
      <Button type="submit" size="sm" disabled={!url.trim()}>
        Старт
      </Button>
    </form>
  );
}
