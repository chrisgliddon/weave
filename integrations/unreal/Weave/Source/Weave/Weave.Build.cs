using System.IO;
using UnrealBuildTool;

public class Weave : ModuleRules
{
	public Weave(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PublicDependencyModuleNames.AddRange(new[] { "Core", "CoreUObject", "Engine" });
		PrivateDependencyModuleNames.AddRange(new[] { "Json", "Projects" });

		// The Weave runtime is a Rust cdylib (crates/weave-ffi), staged by integrations/unreal/stage-ffi.sh.
		string Dir = Path.Combine(PluginDirectory, "Binaries", "ThirdParty", "Weave");
		if (Target.Platform == UnrealTargetPlatform.Linux)
		{
			RuntimeDependencies.Add(Path.Combine(Dir, "Linux", "libweave_ffi.so"));
		}
		else if (Target.Platform == UnrealTargetPlatform.Mac)
		{
			RuntimeDependencies.Add(Path.Combine(Dir, "Mac", "libweave_ffi.dylib"));
		}
		else if (Target.Platform == UnrealTargetPlatform.Win64)
		{
			RuntimeDependencies.Add(Path.Combine(Dir, "Win64", "weave_ffi.dll"));
		}
	}
}
