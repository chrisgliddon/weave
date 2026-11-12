#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"
#include "WeaveStoryPlayer.generated.h"

class UWeaveStory;

UENUM(BlueprintType)
enum class EWeaveFrameKind : uint8
{
	Line,
	Choices,
	Ended,
	Error
};

/** A styled range of a line's text. Offsets are UTF-16 code units into Text, as FString indexes them. */
USTRUCT(BlueprintType)
struct WEAVE_API FWeaveStyleSpan
{
	GENERATED_BODY()

	/** Style name, as written in `<style>`; the Rich Text Block row to use. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Style;

	/** First styled character (inclusive). */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	int32 From = 0;

	/** End of the styled range (exclusive). */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	int32 To = 0;
};

/** A timed pause inside a line, written `<pause N>`. */
USTRUCT(BlueprintType)
struct WEAVE_API FWeaveBeat
{
	GENERATED_BODY()

	/** Pause before revealing the character at this UTF-16 offset into Text. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	int32 At = 0;

	/** Seconds to hold. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	float Seconds = 0.f;
};

/** How a channel presents its lines, as declared with `channel name { ... }`. */
USTRUCT(BlueprintType)
struct WEAVE_API FWeaveChannel
{
	GENERATED_BODY()

	/** Type the text out; false shows a line all at once. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	bool bAnimated = true;

	/** Whether the channel sets its own typing speed. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	bool bHasCharactersPerSecond = false;

	/** Typing speed, when bHasCharactersPerSecond. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	float CharactersPerSecond = 0.f;

	/** Other settings, rendered as text. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TMap<FString, FString> Meta;
};

/** An interface string from a `STRINGS` table. */
USTRUCT(BlueprintType)
struct WEAVE_API FWeaveString
{
	GENERATED_BODY()

	/** Plain text with markup removed. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Text;

	/** Rich Text Block markup: `<style>text</>`. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString RichText;

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FWeaveStyleSpan> Spans;

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FWeaveBeat> Beats;

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FString> Tags;

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TMap<FString, FString> Meta;
};

USTRUCT(BlueprintType)
struct WEAVE_API FWeaveChoice
{
	GENERATED_BODY()

	/** Stable compiler-generated identifier. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Id;

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Text;

	/** The choice disappears once selected. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	bool bOnce = false;

	/** Bare `^tags` on the choice, in source order. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FString> Tags;

	/** `^key:value` tags on the choice. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TMap<FString, FString> Meta;
};

USTRUCT(BlueprintType)
struct WEAVE_API FWeaveFrame
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	EWeaveFrameKind Kind = EWeaveFrameKind::Ended;

	/** The line, for Kind == Line: plain text with markup and tags removed. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Text;

	/** Stable line id (`^id:` or `<knot>.<n>`), for localization and voice lookup. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Id;

	/** Channel that receives the line (`@to`); `narration` by default. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Channel;

	/** Current speaker (`@speaker`); empty while narrating. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Speaker;

	/** Bare `^tags` on the line, in source order. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FString> Tags;

	/** `^key:value` tags on the line. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TMap<FString, FString> Meta;

	/** Rich Text Block markup for the line: `<style>text</>`, with `&<>"` escaped. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString RichText;

	/** Styled ranges of Text (UTF-16 offsets). */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FWeaveStyleSpan> Spans;

	/** Timed pauses inside Text (UTF-16 offsets), in order. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FWeaveBeat> Beats;

	/** The choices, for Kind == Choices. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FWeaveChoice> Choices;

	/** Presentation cues (scene, show, speaker, transition...) as a JSON array, in source order. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString PresentationJson;

	/** Pattern draws as a JSON array. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString DrawsJson;

	/** The error, for Kind == Error. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Error;
};

/** Plays a UWeaveStory with the shared Weave runtime: same semantics as the Bevy and browser hosts. */
UCLASS(BlueprintType)
class WEAVE_API UWeaveStoryPlayer : public UObject
{
	GENERATED_BODY()

public:
	/** Create a player. Returns null and sets Error on failure. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	static UWeaveStoryPlayer* CreateWeaveStoryPlayer(UWeaveStory* Story, int64 Seed, FString& Error);

	/** Advance to the next line, choice set or ending. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	FWeaveFrame Continue();

	UFUNCTION(BlueprintCallable, Category = "Weave")
	TArray<FWeaveChoice> GetChoices();

	/** Select a currently eligible choice by zero-based index. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	bool Choose(int32 Index, FString& Error);

	UFUNCTION(BlueprintPure, Category = "Weave")
	bool CanContinue() const;

	UFUNCTION(BlueprintCallable, Category = "Weave")
	bool Restart(int64 Seed, FString& Error);

	/** Versioned state JSON for save games. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	FString SaveState(FString& Error);

	UFUNCTION(BlueprintCallable, Category = "Weave")
	bool RestoreState(const FString& State, FString& Error);

	/** Look up an interface string from a `STRINGS` table. An unknown table or key sets Error and returns an empty entry. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	FWeaveString GetString(const FString& Table, const FString& Key, FString& Error);

	/** Every declared channel by name; `narration` is always present. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	TMap<FString, FWeaveChannel> GetChannels();

	virtual void BeginDestroy() override;

private:
	void* Handle = nullptr;
};
