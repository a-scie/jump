$mypath = $MyInvocation.MyCommand.Path
$mydir = Split-Path $mypath -Parent

function Basename {
    param (
        $Path
    )
    Write-Output $Path.Substring($Path.LastIndexOf("/") + 1)
}

ForEach ($arg in $args) {
    $example = Basename -Path $arg
    $lines = Get-Content -Path $mydir\$example.fetch
    ForEach ($line in $lines) {
        $url = $line.Trim()
        if ($url) {
            $outfile = Basename -Path $url
            Write-Output "Fetching $url ..."
            Invoke-WebRequest -Uri $url -OutFile $mydir\$example\$outfile
        }
    }
}
