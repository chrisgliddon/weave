#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "WeaveStoryPlayer.h"
#include "WeaveStoryTextActor.generated.h"

class UTextRenderComponent;
class UWeaveStory;

/**
 * Plays a Weave story as 3D text in the level: lines type out, choices are listed and one is picked
 * (from ChoicePlan, else seeded), until the story ends. A ready-made way to see a story running.
 *
 * Headless capture: start the game with -WeaveCapture=<dir> (or set CaptureDirectory) to write
 * every rendered frame as a PNG, and -WeaveQuitWhenDone to exit after the ending. Use a fixed
 * frame rate (-benchmark -fps=30) so the frames form a smooth video.
 */
UCLASS()
class WEAVE_API AWeaveStoryTextActor : public AActor
{
	GENERATED_BODY()

public:
	AWeaveStoryTextActor();

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave")
	TObjectPtr<UWeaveStory> Story;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave")
	int64 Seed = 1;

	/** Choice index to take at each choice point, in order. Past the end, choices are seeded random. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave")
	TArray<int32> ChoicePlan;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Timing")
	float CharactersPerSecond = 40.f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Timing")
	float LineHoldSeconds = 1.2f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Timing")
	float ChoiceHoldSeconds = 1.6f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Timing")
	float EndHoldSeconds = 2.0f;

	/** Characters per line before wrapping. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Look")
	int32 WrapColumns = 34;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Look")
	FColor LineColor = FColor(245, 238, 220);

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Look")
	FColor ChoiceColor = FColor(170, 185, 210);

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Look")
	FColor PickedColor = FColor(255, 196, 64);

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Look")
	FString EndText = TEXT("~ fin ~");

	/** Write every frame here as PNG (also -WeaveCapture=<dir>). Empty = off. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Capture")
	FString CaptureDirectory;

	/** Exit the game after the ending (also -WeaveQuitWhenDone). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Weave|Capture")
	bool bQuitWhenDone = false;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Weave")
	TObjectPtr<UTextRenderComponent> LineText;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Weave")
	TObjectPtr<UTextRenderComponent> ChoicesText;

	/** Fired for every frame the story produces (lines, choices, ending, errors). */
	DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnWeaveFrame, const FWeaveFrame&, Frame);
	UPROPERTY(BlueprintAssignable, Category = "Weave")
	FOnWeaveFrame OnFrame;

	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;

private:
	enum class EState : uint8 { Idle, Typing, Holding, Choosing, Ending, Done };

	void Next();
	void ShowChoices(int32 Picked);
	static FString Wrap(const FString& Text, int32 Columns);

	UPROPERTY(Transient)
	TObjectPtr<UWeaveStoryPlayer> Player;

	EState State = EState::Idle;
	FString Current;
	TArray<FWeaveChoice> Choices;
	int32 Picked = 0;
	int32 ChoiceCount = 0;
	float Clock = 0.f;
	float Elapsed = 0.f;
	int32 FrameIndex = 0;
	FRandomStream Rng;
	FVector BaseLocation = FVector::ZeroVector;
};
