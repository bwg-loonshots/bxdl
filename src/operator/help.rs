//! Exact help-only forms. Never scan option values or literal paths for help.
pub(crate) fn for_args(args: &[String]) -> Option<String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let topic = if words.first() == Some(&"help") {
        &words[1..]
    } else if matches!(words.last(), Some(&"--help" | &"-h")) {
        &words[..words.len() - 1]
    } else {
        return None;
    };
    let (usage, note) = match topic {
        ["completion"] | ["completion", "zsh"] => (
            "bxdl completion zsh",
            "zsh 자동완성 스크립트를 표준출력으로 내보냅니다. 비TTY에서도 사용할 수 있으며 --json과 함께 요청하면 사용법 오류입니다.\ncompinit 실행 뒤 source <(bxdl completion zsh)로 현재 셸에 적용하세요. 셸 설정 파일은 자동 수정하지 않습니다.\n명령·옵션·등록 노드 이름·설치 준비 이름을 해당 위치에서 제안합니다. 이름은 입력 중인 --home 또는 Mac 기본 관리 위치에서 읽습니다.\n자동완성은 관리 위치·기록을 쓰거나 노드·엔진·서비스를 호출하지 않습니다. 제안된 이름이 현재 정상 상태나 실행 승인을 뜻하지는 않습니다.\n현재 지원 셸은 zsh이며 bash/fish는 지원하지 않습니다. 설치·사용 예시는 docs/completion.md를 따르세요.",
        ),
        ["nodes"] => (
            "bxdl nodes [--home <관리-루트>] [--json]",
            "저장한 등록 노드 목록을 읽습니다. 이름·표시명·저장 당시 상태와 시각을 보여줍니다.\n예: bxdl nodes\n목록 조회는 폴더를 만들거나 노드·서비스·네트워크를 조회하지 않습니다. 없는 관리 위치는 빈 목록입니다.\n기존 노드 연결: bxdl nodes add local --instance <control-경로>\nMac 기본 위치는 ~/Library/Application Support/BXDL입니다. 다른 OS 또는 별도 목록은 --home을 지정하세요.\n이름 기반 운용: bxdl status local. 기존 --instance 경로도 지원합니다. 이름에는 같은 --home을 지정하세요.",
        ),
        ["nodes", "add"] => (
            "bxdl nodes add <이름> --instance <기존-control-경로> [--home <관리-루트>] [--json]",
            "기존 등록 노드에 명령 이름을 연결합니다. console이나 항목 ID는 필요하지 않습니다.\n예: bxdl nodes add local --instance /path/to/control\n이름은 소문자로 시작하는 1~32자의 소문자·숫자·하이픈입니다. 표시명·engine instanceId와 별개입니다.\n이 명령 자체가 목록 저장 요청이며 추가 문답 없이 관리 폴더와 이름 기록만 저장합니다. 설치·등록·DB·서비스는 변경하지 않습니다.\n다른 대상이 쓰는 이름과 기존 이름 변경은 거절합니다. 동일 이름·동일 대상 재요청은 기록을 다시 쓰지 않습니다.\n기존 목록은 읽을 때 변환하지 않고, 이름을 처음 저장할 때만 새 목록 형식을 사용합니다.\n다음 명령: bxdl nodes (별도 관리 위치에서는 같은 --home 지정). 이름 기반 status/start/stop/logs/diagnose와 기존 --instance를 지원합니다.",
        ),
        ["console"] => (
            "bxdl console [--home <관리-루트>] [--instance <기존-control-경로>]",
            "TTY 전용 홈·목록·기존 자료 조회입니다. --json과 비TTY 실행은 입력/쓰기 전에 거절합니다.\n홈/목록은 폴더를 만들지 않습니다. 명시 경로를 읽은 뒤 y/yes로 승인한 목록 저장만 관리 위치를 준비합니다.\nMac 기본 위치는 ~/Library/Application Support/BXDL입니다. 다른 OS는 --home을 지정하세요.\n등록 노드와 기존 설치/제품초안/네트워크 세션을 연결하며 재등록·형식변환·임의 CLI 실행은 하지 않습니다.\n목록은 저장 기록이며 현재 서비스 상태는 미조회입니다. 새 설치 자료 준비는 홈 1번에서 기존 번호 메뉴로 진행하며 직접 setup new/resume과 저장 형식을 공유합니다. console의 추가 개발은 중단했습니다. 직접 CLI 개선을 우선합니다.\n:help 도움말, :back 이전 화면, :cancel/EOF 저장 자료를 보존하고 중단. Enter는 변경을 승인하지 않습니다.",
        ),
        ["setup", "new"] => (
            "bxdl setup new <이름> [--home <관리-루트>] [--from <instance.json>]",
            "이름을 붙인 설치 준비 세션을 만듭니다. 이름은 소문자로 시작하는 1~32자의 소문자·숫자·하이픈입니다.\nTTY 전용이며 --json/--non-interactive를 지원하지 않습니다. y/yes 승인 전에는 관리 폴더나 세션을 만들지 않습니다.\n공급 자료→네트워크→누락 제품·정책→검토 순서로 질문하고 새 설정 revision을 준비합니다. 이미 저장한 값은 재사용합니다. 설치·등록·초기화·시작은 수행하지 않습니다.\n중단한 작업은 setup resume <이름>으로 이어갑니다. Mac 기본 관리 위치는 ~/Library/Application Support/BXDL이며 다른 OS는 --home을 지정하세요.",
        ),
        ["setup", "resume"] => (
            "bxdl setup resume <이름> [--home <관리-루트>]",
            "setup new 또는 console에서 만든 준비 세션을 엽니다. 열기만으로 저장·재검증 결과 승인·설치 실행을 하지 않습니다.\n기존 setup --install/--network/제품 초안은 해당 기존 --resume 명령으로 엽니다.\n:help 도움말, :back 이전 질문, :cancel/EOF 저장한 자료를 보존하고 중단. 부족한 입력부터 순서대로 이어갑니다.",
        ),
        ["setup", "list"] => (
            "bxdl setup list [--home <관리-루트>] [--json]",
            "이름을 붙인 설치 준비 목록·남은 입력과 별도 설치 실행 기록을 읽습니다. 없는 관리 위치는 빈 목록이며 폴더·잠금·기록을 쓰지 않습니다.\n목록은 저장 기록이며 공급 자료·엔진·서비스를 재검증하지 않습니다. 손상된 세션은 읽기 불가로 남기고 나머지 목록을 보여줍니다.\n다음 명령: bxdl setup show <이름> (같은 --home 사용).",
        ),
        ["setup", "show"] => (
            "bxdl setup show <이름> [--home <관리-루트>] [--json]",
            "준비한 공개 설정·남은 입력과 별도 설치 실행 기록을 읽습니다. credential 값·경로·비밀 원문은 표시하지 않습니다.\n저장한 PREPARED_RECORDED와 현재 자료 재검증 결과를 분리합니다. 자료가 달라지면 INCOMPLETE이며 기존 기록을 수정하지 않습니다. 설치 완료 기록도 현재 서비스 상태는 아니며 status로 확인하세요.\n읽기 전용이며 workflow 잠금을 획득하거나 엔진·네트워크·서비스를 호출하지 않습니다.\n다음 명령: bxdl setup resume <이름> 또는 bxdl setup edit <이름> --field httpPort (같은 --home 사용).",
        ),
        ["setup", "apply"] => (
            "bxdl setup apply <이름> [--home <관리-루트>]",
            "준비한 자료와 설치 범위를 검토한 뒤 y/yes로 설치·cold 검사·등록·노드 이름 연결을 승인합니다. Enter/n/no는 거절하고 :cancel/EOF는 중단합니다.\nTTY 전용이며 --json/비TTY는 입력·쓰기 전에 거절합니다. 초기화와 시작은 포함하지 않습니다.\n준비 기록 sessions/<이름>과 별도로 operations/<이름>에 설치 실행 기록을 보존합니다. setup list/show에서 두 기록을 구분해 조회하고 현재 실행 상태는 status로 확인하세요.\n완료한 apply를 다시 실행하면 기존 완료 근거를 검증하고 필요한 이름 연결만 복구합니다. 자료 변경·UNKNOWN에서 자동 재설치하거나 기존 폴더를 채택하지 않습니다.\n이름 연결이 실패해도 이미 완료한 설치·등록은 보존합니다. 안내된 control 경로로 조회하고 같은 apply의 복구 안내를 따르세요.\n다음 명령: bxdl init <이름> --confirm-initialize, 이후 bxdl start <이름> (별도 관리 위치에서는 같은 --home 지정).",
        ),
        ["setup", "edit"] => (
            "bxdl setup edit <이름> --field <제품-항목> [--home <관리-루트>]",
            "지정한 제품 항목 하나만 질문하고 종료합니다. 저장한 값에서 Enter는 유지하고 :back은 변경 없이 종료합니다. :cancel/EOF는 기존 값을 보존하고 중단합니다.\nTTY 전용이며 --json/비TTY는 입력·쓰기 전에 거절합니다. 같은 값은 새 기록을 쓰지 않습니다. 실제 변경은 기존 검토를 무효화하되 이전 생성 파일을 보존합니다.\n지원 항목: instanceId, nodeId, dataDirectory, httpAddress, httpPort, p2pAddress, p2pPort, chainDescription, validatorKeystore, validatorPasswordFile, tlsKeyStore, tlsKeyPasswordFile, tlsTrustStore, tlsTrustPasswordFile.\n비밀번호 원문 대신 기존 파일 경로를 입력하세요. 공급 자료·정책 수정은 setup resume <이름>의 검토에서 진행합니다.\n다음 명령: bxdl setup resume <이름> (같은 --home 사용). 설치·등록·초기화·시작은 수행하지 않습니다.",
        ),
        ["setup"] => (
            "bxdl setup [--workspace <dir>] [--resume | --from <instance.json>] [--output <new.json>]\n  bxdl setup --workspace <dir> (--from <instance.json> | --resume) --non-interactive [--output <new.json>] [--json]",
            "제품 설정 초안·로컬 검사·새 설정 파일을 준비합니다. 설치·초기화·시작은 수행하지 않습니다.\n대화형 입력: :help 도움말, :back 이전 항목/검토, :cancel 저장 후 중단. edit은 번호로 한 항목만 수정합니다.\n대화형 새 설정 파일 출력은 y/yes만 승인하며 확인의 Enter는 거절합니다. 비대화형 모드는 표준입력을 읽지 않습니다.",
        ),
        ["setup", "--install"] => (
            "bxdl setup --install --workspace <new-session> [--from <instance.json>]\n  bxdl setup --install --workspace <session> --resume",
            "macOS arm64 터미널에서 실행합니다. 지원하는 RC.3 패키지는 독립 관리 CLI를 사용하고 그 외에는 기존 동일 CLI 조건을 유지합니다. 부모 폴더는 미리 준비하세요.\narchive·외부 신뢰키·engine lock·native 설정과 기존 키 자료를 준비합니다.\ninputs/edit으로 항목 하나를 수정하고 plan으로 재검토합니다. apply·init·start는 각각 별도로 y/yes 확인을 받습니다.\n:help 도움말, :back 이전 항목/검토, :cancel/EOF 저장 후 중단. Enter는 실행을 승인하지 않습니다.\n완료 세션의 generated 제품 JSON은 운용 입력이므로 보존하세요. --json/--non-interactive는 지원하지 않습니다.",
        ),
        ["setup", "--network"] => (
            "bxdl setup --network --workspace <new-session> --from <instance.json> --network-file <network.json> [--output <new-node.json>]\n  bxdl setup --network --workspace <session> --resume [--output <new-node.json>]",
            "승인된 공개 목록과 기존 제품 설정으로 native node.json을 준비합니다. 키 발급·cold 검사·노드 실행은 하지 않습니다.\n:help 도움말, :back 이전 항목/검토, :cancel/EOF 저장 후 중단. edit은 번호로 한 정책만 수정합니다.\n출력은 새 파일에만 y/yes 확인 후 저장하며 Enter는 거절합니다. --json/--non-interactive는 지원하지 않습니다.",
        ),
        ["start"] => (
            "bxdl start <이름> [--home <관리-루트>] [기존 옵션]\n  bxdl start --instance <control-dir> [--timeout-seconds <1..600>] [--json]",
            "등록·초기화한 노드를 현재 Mac 사용자의 LaunchAgent로 명시 시작합니다. 지원하는 RC.3 패키지는 검증된 내부 실행기에 위임합니다. 관리 CLI를 바꾸어도 기존 패키지는 유지합니다. 그 외 패키지는 기존 동일 CLI 조건을 유지합니다.\n기본 관측 시간은 120초입니다. 시간이 끝나도 실행은 계속될 수 있으므로 status --wait로 같은 시도를 확인하세요.\n로그인 자동 시작·자동 재시작은 하지 않습니다. 터미널 종료와 달리 사용자 로그아웃은 노드를 종료합니다.",
        ),
        ["status"] => (
            "bxdl status <이름> [--home <관리-루트>] [기존 옵션]\n  bxdl status --instance <control-dir> [--timeout-seconds <1..120>] [--json]\n  bxdl status --instance <control-dir> --wait [--timeout-seconds <1..600>] [--json]",
            "현재 시도의 서비스·엔진·로컬 준비 상태를 관측합니다. 초기화·시작·중지는 수행하지 않습니다.\n단일 조회 기본 10초, --wait 기본 120초입니다. 대기 만료는 시작 실패나 종료 확정이 아닙니다.\n로컬 READY만으로 네트워크 합의·거래 확정을 보장하지 않습니다.",
        ),
        ["stop"] => (
            "bxdl stop <이름> [--home <관리-루트>] [기존 옵션]\n  bxdl stop --instance <control-dir> [--timeout-seconds <1..600>] [--json]",
            "등록한 현재 시작 시도의 정상 종료를 요청하고 확인합니다. 기본 제한 시간은 60초입니다.\n종료 대기 만료·결과 불명에서는 새 start를 반복하지 말고 status와 정제 기록을 확인하세요.\nSERVICE_STOPPED_VERIFIED 이후에도 다음 start는 별도로 명시해야 합니다. 데이터는 삭제하지 않습니다.",
        ),
        ["init"] => (
            "bxdl init <이름> [--home <관리-루트>] --confirm-initialize [--timeout-seconds <1..600>] [--json]\n  bxdl init --instance <control-dir> --confirm-initialize [--timeout-seconds <1..600>] [--json]",
            "등록된 새 데이터 경로를 한 번 초기화합니다. 노드·서비스를 시작하지 않습니다. 이름과 --instance는 둘 중 하나만 받으며 --home은 이름에만 사용합니다.\n체인·자산 등록 정책을 먼저 검토하세요. 실패·불명 결과에서 init을 자동 반복하지 않습니다.",
        ),
        ["resume-init"] => (
            "bxdl resume-init <이름> [--home <관리-루트>] --confirm-resume [--timeout-seconds <1..600>] [--json]\n  bxdl resume-init --instance <control-dir> --confirm-resume [--timeout-seconds <1..600>] [--json]",
            "미완료 초기화의 재개 조건을 검사하고 조건이 맞을 때만 이어갑니다. 일반적인 UNKNOWN 복구 명령이 아닙니다. 이름과 --instance는 둘 중 하나만 받으며 --home은 이름에만 사용합니다.\n기존 기록을 삭제하거나 DB를 재초기화하지 않습니다. 노드를 시작하지 않습니다.",
        ),
        ["logs"] => (
            "bxdl logs <이름> [--home <관리-루트>] [기존 옵션]\n  bxdl logs --instance <control-dir> [--tail <1..200>] [--max-bytes <4096..1048576>] [--timeout-seconds <1..30>] [--json]",
            "최신 초기화·서비스 시도의 정제한 저장 기록을 읽습니다. live 상태 확인·raw 출력·follow는 제공하지 않습니다.\n기본 한도: tail 50개, 262144 bytes, 5초. 누락·한도 초과는 partial로 표시합니다.",
        ),
        ["diagnose"] => (
            "bxdl diagnose <이름> [--home <관리-루트>] --output <new.json> [기존 옵션]\n  bxdl diagnose --instance <control-dir> --output <new.json> [--tail <1..200>] [--max-bytes <4096..1048576>] [--timeout-seconds <1..30>] [--json]",
            "정제한 저장 기록을 새 JSON 지원 보고서에 저장합니다. 기존 파일을 덮어쓰거나 자동 전송하지 않습니다.\nJVM·네트워크·서비스 호출 없이 수집하므로 현재 실행 상태를 검증한 보고서가 아닙니다.",
        ),
        ["preflight"] => (
            "bxdl preflight --config <instance.json> [--json]\n  bxdl preflight --config <instance.json> --engine-config <node.json> --jar <jar> --java <java> --lock <json> --allow-development [--timeout-seconds <1..120>] [--json]\n  bxdl preflight --instance <control-dir> [--timeout-seconds <1..120>] [--json]",
            "기본은 로컬 정적 검사입니다. 엔진 옵션을 모두 지정하거나 등록 인스턴스를 지정하면 cold 검사를 수행합니다.\n알려진 cold INCOMPLETE는 DB·서비스·runtime 검사가 남았다는 뜻이며 로컬 READY가 아닙니다. 초기화·시작은 하지 않습니다.",
        ),
        ["install"] => (
            "bxdl install <tar.gz> --destination <new-dir> (--public-key <trusted.pem> | --allow-unsigned-development) [--json]",
            "macOS arm64의 새 폴더에 검증한 package 파일만 설치합니다. 부모 폴더는 미리 준비하세요.\n신뢰키는 패키지 밖의 독립적으로 신뢰한 경로에서 제공합니다. 등록·초기화·서비스 시작은 하지 않습니다.\n이름 있는 준비 자료의 설치는 bxdl setup apply --help, 기존 설치 문답은 bxdl setup --install --help를 확인하세요.",
        ),
        ["instance", "register"] => (
            "bxdl instance register --instance <new-dir> --package <installed-dir> --archive <tar.gz> (--public-key <trusted.pem> | --allow-unsigned-development) --config <instance.json> --engine-config <node.json> --lock <json> --allow-development [--timeout-seconds <1..120>] [--json]",
            "새 control 폴더에 package·설정·cold 검사 결과를 결속합니다. 기존 DB를 채택하거나 초기화·시작하지 않습니다.\n새 V2 등록은 archive·공개키·lock을 trust/에 보관합니다. 제품/native/chain·credential 참조는 계속 보존하세요. 기존 V1은 자동 변환하지 않습니다.",
        ),
        ["instance", "show"] => (
            "bxdl instance show --instance <control-dir> [--json]",
            "등록 기록을 조회합니다. --instance는 이름이 아닌 control 폴더 경로입니다. 현재 runtime 상태는 status로 별도 관측하세요.",
        ),
        ["config", "validate"] => (
            "bxdl config validate --file <instance.json> [--json]",
            "제품 설정의 형식을 검사합니다. 키 내용·native 설정·runtime 준비 상태 검사를 대신하지 않습니다.",
        ),
        ["config", "fingerprint"] => (
            "bxdl config fingerprint --certificate <leaf.pem|leaf.der> [--json]",
            "공개 인증서 bytes에서 NIGO MTLS pin을 계산합니다. 인증서 신뢰·만료·keystore 일치를 검증하지 않습니다. 개인키를 입력하지 마세요.",
        ),
        ["engine", "inspect"] => (
            "bxdl engine inspect --jar <jar> --java <java> --lock <json> --allow-development [--timeout-seconds <1..120>] [--json]",
            "고정 engine/JRE identity와 engine-info를 검사합니다. DB 초기화·서비스 시작·제품 호환성 전체 인수는 별도입니다.",
        ),
        ["engine", "preflight"] => (
            "bxdl engine preflight --jar <jar> --java <java> --lock <json> --config <node.json> --allow-development [--timeout-seconds <1..120>] [--json]",
            "NIGO native node.json으로 cold 검사를 수행합니다. 제품 instance.json이 아닙니다.\n알려진 INCOMPLETE도 그대로 반환하며 DB 초기화·노드 시작은 수행하지 않습니다.",
        ),
        ["package", "verify"] => (
            "bxdl package verify <tar.gz> (--public-key <trusted.pem> | --allow-unsigned-development) [--json]",
            "archive의 서명·전체 inventory를 검사합니다. 신뢰키는 package 밖에서 독립적으로 확보하세요.\n엔진 실행·공식 공급 승인·호스트 서비스 인수를 뜻하지 않습니다.",
        ),
        ["package", "build"] => (
            "bxdl package build --root <dir> --spec <json> --output <tar.gz> (--signing-key <private.pem> | --allow-unsigned-development) [--json]",
            "명시된 payload/spec으로 새 archive를 만듭니다. 서명 없는 개발 자료는 별도 opt-in이 필요합니다.\nCLI 변경 시 archive identity가 달라지며 새 package 인수가 필요합니다.",
        ),
        ["runtime", "stage"] => (
            "bxdl runtime stage --source <extracted-java-home> --lock <json> --output <new-dir> [--json]",
            "신뢰 lock의 runtime inventory를 새 디렉터리에 준비합니다. 원본 archive 인증·JVM 실행·제품 호환성 인수는 별도입니다.",
        ),
        ["version"] => (
            "bxdl version [--json]",
            "CLI 버전·revision·구현 범위를 표시합니다. 같은 버전 문자열이 같은 CLI bytes를 보장하지 않습니다.",
        ),
        _ => return None,
    };
    Some(format!("BXDL 명령 안내\n\n사용법:\n  {usage}\n\n{note}\n"))
}
