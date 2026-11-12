#include "WeaveStoryTextActor.h"

#include "Components/TextRenderComponent.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "UnrealClient.h"
#include "WeaveStory.h"

AWeaveStoryTextActor::AWeaveStoryTextActor()
{
	PrimaryActorTick.bCanEverTick = true;
	RootComponent = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));

	LineText = CreateDefaultSubobject<UTextRenderComponent>(TEXT("LineText"));
	LineText->SetupAttachment(RootComponent);
	LineText->SetHorizontalAlignment(EHTA_Center);
	LineText->SetVerticalAlignment(EVRTA_TextBottom);
	LineText->SetWorldSize(34.f);
	LineText->SetText(FText::GetEmpty());

	ChoicesText = CreateDefaultSubobject<UTextRenderComponent>(TEXT("ChoicesText"));
	ChoicesText->SetupAttachment(RootComponent);
	ChoicesText->SetHorizontalAlignment(EHTA_Center);
	ChoicesText->SetVerticalAlignment(EVRTA_TextTop);
	ChoicesText->SetWorldSize(26.f);
	ChoicesText->SetRelativeLocation(FVector(0.f, 0.f, -20.f));
	ChoicesText->SetText(FText::GetEmpty());
}

void AWeaveStoryTextActor::BeginPlay()
{
	Super::BeginPlay();
	BaseLocation = GetActorLocation();
	Rng.Initialize(static_cast<int32>(Seed));

	FString Dir;
	if (FParse::Value(FCommandLine::Get(), TEXT("WeaveCapture="), Dir))
	{
		CaptureDirectory = Dir;
	}
	if (FParse::Param(FCommandLine::Get(), TEXT("WeaveQuitWhenDone")))
	{
		bQuitWhenDone = true;
	}

	FString Error;
	Player = UWeaveStoryPlayer::CreateWeaveStoryPlayer(Story, Seed, Error);
	if (!Player)
	{
		LineText->SetText(FText::FromString(TEXT("Weave: ") + Error));
		UE_LOG(LogTemp, Error, TEXT("Weave: %s"), *Error);
		State = EState::Ending; // show the error briefly, then quit if asked, so batch renders never hang
		return;
	}
	Next();
}

FString AWeaveStoryTextActor::Wrap(const FString& Text, int32 Columns)
{
	TArray<FString> Words;
	Text.ParseIntoArrayWS(Words);
	FString Out, Line;
	for (const FString& W : Words)
	{
		if (!Line.IsEmpty() && Line.Len() + 1 + W.Len() > Columns)
		{
			Out += Line + TEXT("\n");
			Line.Reset();
		}
		Line += Line.IsEmpty() ? W : TEXT(" ") + W;
	}
	return Out + Line;
}

void AWeaveStoryTextActor::Next()
{
	const FWeaveFrame Frame = Player->Continue();
	OnFrame.Broadcast(Frame);
	Clock = 0.f;
	switch (Frame.Kind)
	{
	case EWeaveFrameKind::Line:
		Current = Wrap(Frame.Text, WrapColumns);
		ChoicesText->SetText(FText::GetEmpty());
		LineText->SetTextRenderColor(LineColor);
		State = EState::Typing;
		break;
	case EWeaveFrameKind::Choices:
		Choices = Frame.Choices;
		Picked = ChoicePlan.IsValidIndex(ChoiceCount) ? FMath::Clamp(ChoicePlan[ChoiceCount], 0, Choices.Num() - 1)
		                                              : Rng.RandRange(0, Choices.Num() - 1);
		++ChoiceCount;
		ShowChoices(INDEX_NONE);
		State = EState::Choosing;
		break;
	case EWeaveFrameKind::Ended:
		Current = EndText;
		LineText->SetText(FText::FromString(EndText));
		LineText->SetTextRenderColor(PickedColor);
		ChoicesText->SetText(FText::GetEmpty());
		State = EState::Ending;
		break;
	default:
		LineText->SetText(FText::FromString(TEXT("Weave error: ") + Frame.Error));
		UE_LOG(LogTemp, Error, TEXT("Weave: %s"), *Frame.Error);
		State = EState::Ending;
		break;
	}
}

void AWeaveStoryTextActor::ShowChoices(int32 Highlight)
{
	FString Text;
	for (int32 i = 0; i < Choices.Num(); ++i)
	{
		Text += (i == Highlight ? TEXT("> ") : TEXT("  ")) + Choices[i].Text + (i == Highlight ? TEXT(" <") : TEXT("  ")) + TEXT("\n");
	}
	ChoicesText->SetText(FText::FromString(Text));
	ChoicesText->SetTextRenderColor(Highlight == INDEX_NONE ? ChoiceColor : PickedColor);
}

void AWeaveStoryTextActor::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);
	Clock += DeltaSeconds;
	Elapsed += DeltaSeconds;

	// A gentle float so the scene is never static.
	SetActorLocation(BaseLocation + FVector(0.f, 0.f, 6.f * FMath::Sin(Elapsed * 1.3f)));

	switch (State)
	{
	case EState::Typing:
	{
		const int32 Shown = FMath::Min(Current.Len(), FMath::FloorToInt(Clock * CharactersPerSecond));
		LineText->SetText(FText::FromString(Current.Left(Shown)));
		if (Shown >= Current.Len())
		{
			State = EState::Holding;
			Clock = 0.f;
		}
		break;
	}
	case EState::Holding:
		if (Clock >= LineHoldSeconds)
		{
			Next();
		}
		break;
	case EState::Choosing:
		if (Clock >= ChoiceHoldSeconds * 0.6f)
		{
			ShowChoices(Picked);
		}
		if (Clock >= ChoiceHoldSeconds)
		{
			FString Error;
			Player->Choose(Picked, Error);
			LineText->SetText(FText::GetEmpty());
			Next();
		}
		break;
	case EState::Ending:
		if (Clock >= EndHoldSeconds)
		{
			State = EState::Done;
			if (bQuitWhenDone)
			{
				FPlatformMisc::RequestExit(false);
			}
		}
		break;
	default:
		break;
	}

	if (!CaptureDirectory.IsEmpty() && State != EState::Done)
	{
		FScreenshotRequest::RequestScreenshot(
			FPaths::Combine(CaptureDirectory, FString::Printf(TEXT("frame_%05d.png"), FrameIndex++)), false, false);
	}
}
