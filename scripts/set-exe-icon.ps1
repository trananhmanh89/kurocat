<#
.SYNOPSIS
    Writes the app icon into KuroCat.exe after a build.

.DESCRIPTION
    Cargo cannot embed the icon on machines without a resource compiler (rc.exe or windres), so
    run this after every build. It uses the Windows resource update API, so nothing has to be
    installed. The icon is stored inside the exe and shows on any machine.

    If the exe is code-signed, run this before signing.

.PARAMETER ExePath
    The exe to update. Default: target\release\KuroCat.exe

.PARAMETER IconPath
    The .ico file to embed. Default: assets\icons\icon.ico

.EXAMPLE
    cargo build --release
    pwsh scripts/set-exe-icon.ps1
#>
param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\KuroCat.exe"),
    [string]$IconPath = (Join-Path $PSScriptRoot "..\assets\icons\icon.ico")
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Runtime.InteropServices;

public static class ExeIconWriter
{
    private const int ResourceTypeIcon = 3;
    private const int ResourceTypeGroupIcon = 14;
    private const ushort NeutralLanguage = 0;

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern IntPtr BeginUpdateResource(string fileName, bool deleteExistingResources);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool UpdateResource(IntPtr update, IntPtr type, IntPtr name, ushort language, byte[] data, uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool EndUpdateResource(IntPtr update, bool discardChanges);

    /// <summary>Replaces the main icon of an exe with the images of an .ico file.</summary>
    /// <param name="exePath">The exe to update.</param>
    /// <param name="iconPath">The .ico file to embed.</param>
    /// <returns>The number of icon images written.</returns>
    public static int Write(string exePath, string iconPath)
    {
        byte[] icon = File.ReadAllBytes(iconPath);
        ushort imageCount = BitConverter.ToUInt16(icon, 4);

        IntPtr update = BeginUpdateResource(exePath, false);
        if (update == IntPtr.Zero)
        {
            throw new InvalidOperationException("BeginUpdateResource failed, error " + Marshal.GetLastWin32Error());
        }

        bool finished = false;
        try
        {
            // The group table: a 6-byte header, then 14 bytes per image.
            byte[] group = new byte[6 + 14 * imageCount];
            Array.Copy(icon, 0, group, 0, 6);

            for (int index = 0; index < imageCount; index++)
            {
                int entryOffset = 6 + 16 * index;
                int imageSize = BitConverter.ToInt32(icon, entryOffset + 8);
                int imageOffset = BitConverter.ToInt32(icon, entryOffset + 12);
                ushort resourceId = (ushort)(index + 1);

                byte[] image = new byte[imageSize];
                Array.Copy(icon, imageOffset, image, 0, imageSize);
                Check(UpdateResource(update, (IntPtr)ResourceTypeIcon, (IntPtr)resourceId, NeutralLanguage, image, (uint)imageSize), "icon image");

                // Same first 12 bytes as the .ico entry, then the resource id instead of the file offset.
                int groupOffset = 6 + 14 * index;
                Array.Copy(icon, entryOffset, group, groupOffset, 12);
                group[groupOffset + 12] = (byte)(resourceId & 0xFF);
                group[groupOffset + 13] = (byte)(resourceId >> 8);
            }

            Check(UpdateResource(update, (IntPtr)ResourceTypeGroupIcon, (IntPtr)1, NeutralLanguage, group, (uint)group.Length), "icon group");
            finished = true;
        }
        finally
        {
            if (!EndUpdateResource(update, !finished) && finished)
            {
                throw new InvalidOperationException("EndUpdateResource failed, error " + Marshal.GetLastWin32Error());
            }
        }
        return imageCount;
    }

    /// <summary>Throws when a resource update call failed.</summary>
    /// <param name="succeeded">The result of the call.</param>
    /// <param name="what">What was being written, for the error message.</param>
    private static void Check(bool succeeded, string what)
    {
        if (!succeeded)
        {
            throw new InvalidOperationException("UpdateResource failed for " + what + ", error " + Marshal.GetLastWin32Error());
        }
    }
}
"@

$exe = (Resolve-Path $ExePath).Path
$icon = (Resolve-Path $IconPath).Path
$count = [ExeIconWriter]::Write($exe, $icon)
Write-Host "Embedded $count icon images from $icon into $exe"
