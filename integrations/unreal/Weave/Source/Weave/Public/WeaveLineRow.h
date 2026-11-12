#pragma once

#include "CoreMinimal.h"
#include "Engine/DataTable.h"
#include "WeaveLineRow.generated.h"

/**
 * One narrative line as written by `weavec export --unreal`: import Lines.csv or Lines.json as a
 * DataTable with this row struct. The row name is the stable line id.
 */
USTRUCT(BlueprintType)
struct WEAVE_API FWeaveLineRow : public FTableRowBase
{
	GENERATED_BODY()

	/** Knot that contains the line. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Knot;

	/** Channel the line plays on, as far as source order determines it. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Channel;

	/** Speaker, as far as source order determines it; empty for narration. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Speaker;

	/** Plain text with markup removed; `{expr}` and `#grammar#` stay as placeholders. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Text;

	/** Rich Text Block markup for the line. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Rich;

	/** Bare tags, space separated. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Tags;

	/** `key=value` tags, separated by `;`. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString Meta;

	/** Suggested voice asset name. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString VoFile;
};
