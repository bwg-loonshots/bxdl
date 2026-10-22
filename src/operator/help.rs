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
        ["setup"] => (
            "bxdl setup [--workspace <dir>] [--resume | --from <instance.json>] [--output <new.json>]\n  bxdl setup --workspace <dir> (--from <instance.json> | --resume) --non-interactive [--output <new.json>] [--json]",
            "제품 설정 초안·로컬 검사·새 설정 파일을 준비합니다. 설치·초기화·시작은 수행하지 않습니다.\n대화형 입력: :help 도움말, :back 이전 항목/검토, :cancel 저장 후 중단. edit은 번호로 한 항목만 수정합니다.\n대화형 새 설정 파일 출력은 y/yes만 승인하며 확인의 Enter는 거절합니다. 비대화형 모드는 표준입력을 읽지 않습니다.",
        ),
        ["setup", "--install"] => (
            "bxdl setup --install --workspace <new-session> [--from <instance.json>]\n  bxdl setup --install --workspace <session> --resume",
            "macOS arm64 터미널에서 패키지와 같은 bytes의 CLI를 사용합니다. 부모 폴더는 미리 준비하세요.\narchive·외부 신뢰키·engine lock·native 설정과 기존 키 자료를 준비합니다.\ninputs/edit으로 항목 하나를 수정하고 plan으로 재검토합니다. apply·init·start는 각각 별도로 y/yes 확인을 받습니다.\n:help 도움말, :back 이전 항목/검토, :cancel/EOF 저장 후 중단. Enter는 실행을 승인하지 않습니다.\n완료 세션의 generated 제품 JSON은 운용 입력이므로 보존하세요. --json/--non-interactive는 지원하지 않습니다.",
        ),
        ["setup", "--network"] => (
            "bxdl setup --network --workspace <new-session> --from <instance.json> --network-file <network.json> [--output <new-node.json>]\n  bxdl setup --network --workspace <session> --resume [--output <new-node.json>]",
            "승인된 공개 목록과 기존 제품 설정으로 native node.json을 준비합니다. 키 발급·cold 검사·노드 실행은 하지 않습니다.\n:help 도움말, :back 이전 항목/검토, :cancel/EOF 저장 후 중단. edit은 번호로 한 정책만 수정합니다.\n출력은 새 파일에만 y/yes 확인 후 저장하며 Enter는 거절합니다. --json/--non-interactive는 지원하지 않습니다.",
        ),
        ["start"] => (
            "bxdl start --instance <control-dir> [--timeout-seconds <1..600>] [--json]",
            "등록·초기화한 노드를 현재 Mac 사용자의 LaunchAgent로 명시 시작합니다. 설치 package와 같은 bytes의 CLI가 필요합니다.\n기본 관측 시간은 120초입니다. 시간이 끝나도 실행은 계속될 수 있으므로 status --wait로 같은 시도를 확인하세요.\n로그인 자동 시작·자동 재시작은 하지 않습니다. 터미널 종료와 달리 사용자 로그아웃은 노드를 종료합니다.",
        ),
        ["status"] => (
            "bxdl status --instance <control-dir> [--timeout-seconds <1..120>] [--json]\n  bxdl status --instance <control-dir> --wait [--timeout-seconds <1..600>] [--json]",
            "현재 시도의 서비스·엔진·로컬 준비 상태를 관측합니다. 초기화·시작·중지는 수행하지 않습니다.\n단일 조회 기본 10초, --wait 기본 120초입니다. 대기 만료는 시작 실패나 종료 확정이 아닙니다.\n로컬 READY만으로 네트워크 합의·거래 확정을 보장하지 않습니다.",
        ),
        ["stop"] => (
            "bxdl stop --instance <control-dir> [--timeout-seconds <1..600>] [--json]",
            "등록한 현재 시작 시도의 정상 종료를 요청하고 확인합니다. 기본 제한 시간은 60초입니다.\n종료 대기 만료·결과 불명에서는 새 start를 반복하지 말고 status와 정제 기록을 확인하세요.\nSERVICE_STOPPED_VERIFIED 이후에도 다음 start는 별도로 명시해야 합니다. 데이터는 삭제하지 않습니다.",
        ),
        ["init"] => (
            "bxdl init --instance <control-dir> --confirm-initialize [--timeout-seconds <1..600>] [--json]",
            "등록된 새 데이터 경로를 한 번 초기화합니다. 노드·서비스를 시작하지 않습니다.\n체인·자산 등록 정책을 먼저 검토하세요. 실패·불명 결과에서 init을 자동 반복하지 않습니다.",
        ),
        ["resume-init"] => (
            "bxdl resume-init --instance <control-dir> --confirm-resume [--timeout-seconds <1..600>] [--json]",
            "미완료 초기화의 재개 조건을 검사하고 조건이 맞을 때만 이어갑니다. 일반적인 UNKNOWN 복구 명령이 아닙니다.\n기존 기록을 삭제하거나 DB를 재초기화하지 않습니다. 노드를 시작하지 않습니다.",
        ),
        ["logs"] => (
            "bxdl logs --instance <control-dir> [--tail <1..200>] [--max-bytes <4096..1048576>] [--timeout-seconds <1..30>] [--json]",
            "최신 초기화·서비스 시도의 정제한 저장 기록을 읽습니다. live 상태 확인·raw 출력·follow는 제공하지 않습니다.\n기본 한도: tail 50개, 262144 bytes, 5초. 누락·한도 초과는 partial로 표시합니다.",
        ),
        ["diagnose"] => (
            "bxdl diagnose --instance <control-dir> --output <new.json> [--tail <1..200>] [--max-bytes <4096..1048576>] [--timeout-seconds <1..30>] [--json]",
            "정제한 저장 기록을 새 JSON 지원 보고서에 저장합니다. 기존 파일을 덮어쓰거나 자동 전송하지 않습니다.\nJVM·네트워크·서비스 호출 없이 수집하므로 현재 실행 상태를 검증한 보고서가 아닙니다.",
        ),
        ["preflight"] => (
            "bxdl preflight --config <instance.json> [--json]\n  bxdl preflight --config <instance.json> --engine-config <node.json> --jar <jar> --java <java> --lock <json> --allow-development [--timeout-seconds <1..120>] [--json]\n  bxdl preflight --instance <control-dir> [--timeout-seconds <1..120>] [--json]",
            "기본은 로컬 정적 검사입니다. 엔진 옵션을 모두 지정하거나 등록 인스턴스를 지정하면 cold 검사를 수행합니다.\n알려진 cold INCOMPLETE는 DB·서비스·runtime 검사가 남았다는 뜻이며 로컬 READY가 아닙니다. 초기화·시작은 하지 않습니다.",
        ),
        ["install"] => (
            "bxdl install <tar.gz> --destination <new-dir> (--public-key <trusted.pem> | --allow-unsigned-development) [--json]",
            "macOS arm64의 새 폴더에 검증한 package 파일만 설치합니다. 부모 폴더는 미리 준비하세요.\n신뢰키는 패키지 밖의 독립적으로 신뢰한 경로에서 제공합니다. 등록·초기화·서비스 시작은 하지 않습니다.\n문답 설치 흐름은 bxdl setup --install --help를 확인하세요.",
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
