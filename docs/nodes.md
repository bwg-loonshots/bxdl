# 직접 노드 목록과 이름 연결

CLI-01에서 `nodes`와 `nodes add`를 제공한다. console을 먼저 열 필요가 없다. `status/start/stop/logs/diagnose <이름>`으로 연결된 노드를 운용한다. 기존 `--instance` 경로도 계속 사용할 수 있다.

```sh
bxdl nodes add local --instance /path/to/control
bxdl nodes
bxdl status local
```

`control`은 `instance register` 또는 기존 설치 도우미로 만든 등록 폴더다. package 폴더, 제품 JSON 또는 준비 세션을 대신 지정하지 않는다. `nodes add`는 새 엔진 등록 명령이 아니라 기존 등록에 이름을 연결하는 명령이다.

별도 목록 위치와 자동화 출력도 지원한다.

```sh
bxdl nodes add local --instance /path/to/control --home /path/to/bxdl-home --json
bxdl nodes --home /path/to/bxdl-home --json
```

Mac 기본 관리 위치는 `~/Library/Application Support/BXDL`이다. 다른 OS에서는 `--home`을 지정한다. 목록별 이름은 독립적이므로 같은 목록을 보려면 같은 `--home`을 사용한다. 두 명령은 TTY가 필요 없으며 표준입력을 읽지 않는다.

## 이름 연결

이름은 `^[a-z][a-z0-9-]{0,31}$` 형식이다. `local`, `validator-1`처럼 소문자로 시작하는 1~32자의 소문자·숫자·하이픈을 사용한다. 표시명·engine instanceId·불변 항목 ID와 별개다.

`nodes add` 실행 자체가 이름 저장 요청이다. 별도 확인 문답 없이 관리 폴더와 목록만 저장한다. 새 폴더는 0700, 기록은 0600으로 만들며 기존 임의 폴더는 채택하지 않는다. 등록·package·DB·서비스를 변경하거나 JVM을 실행하지 않는다.

- 같은 이름과 같은 대상의 재요청은 성공하며 기록 revision을 다시 쓰지 않는다.
- 다른 대상이 쓰는 이름은 거절한다. 이미 다른 이름을 가진 대상의 이름을 자동 변경하지 않는다.
- 기존 이름 없는 목록 항목에 이름을 붙이면 ID·표시명·저장 당시 상태/시각은 유지한다. 이름과 binding 식별 정보를 추가한다.
- 기존 catalog V1은 조회 시 그대로 읽는다. 이름을 명시 저장할 때 catalog V2로 새 revision을 기록한다. 기존 저장 revision은 보존한다. 구형 BXDL은 새 V2 목록을 읽을 수 없지만 개별 `--instance` 명령은 목록과 독립적이다.
- control 경로·root identity·binding이 바뀌거나 목록이 손상/동시 변경되면 연결을 거절한다. 자동 복구·경로 추정·새 대상 채택은 하지 않는다.

목록 저장 중 실패한 경우 일부 관리 폴더나 기록이 남을 수 있다. 기존 노드는 바뀌지 않는다. 출력 실패의 exit 7 역시 이름 저장이 안 됐다는 뜻은 아니므로 `nodes`로 목록을 확인한다.

## 목록의 의미

`nodes`는 등록 노드 항목만 보여준다. 설치/제품/network 준비 세션은 이 목록에서 제외한다. 없는 관리 위치는 빈 목록이며 부모 폴더도 생성하지 않는다. 기존 목록이 손상됐을 때는 빈 목록으로 숨기지 않고 오류를 반환한다.

출력에는 이름·표시명·instanceId·저장 당시 상태와 시각이 있다. 이름 없는 기존 항목은 불변 ID로 구분한다. `currentStatus`는 항상 `NOT_OBSERVED`다. 현재 대상이 없어졌어도 저장한 목록을 보여주며, 등록 폴더·package·HTTP·서비스를 다시 조회하지 않는다. 현재 실행 상태는 `status <이름>` 또는 `status --instance`로 확인한다.

사람용 출력은 필요한 값만 보여주고 private 경로·binding hash는 출력하지 않는다. JSON은 기존 envelope를 사용한다. `nodes`의 data는 `entries`와 `currentStatus`이고 각 항목은 `id`, nullable `name`, `label`, `instanceId`, `recordedState`, `recordedAt`, `currentStatus`다. `nodes add`는 연결한 항목 하나를 data로 반환한다. envelope의 `observedAt`은 명령 결과 생성 시각이며, 상태 저장 시각은 `recordedAt`이다.

성공은 exit 0(`NODE_LIST`/`NODE_NAME_LINKED`), 잘못된 인자·이름은 2, 대상/목록 검증·저장 거절은 3, 출력 실패는 7이다. 조회 성공은 노드 실행 상태가 정상이라는 뜻이 아니다.

명령별 안내는 `bxdl nodes --help`, `bxdl nodes add --help`다. 최상위 도움말은 내 노드 → 운용 → 설치·준비 → 설정·패키지 제작 순서로 안내한다. [관리 CLI 가이드](./management-cli.md)에 독립 업데이트와 기존 RC.3 실행기 위임 범위를 설명한다. 이름에는 같은 `--home`을 지정하며 `--instance`와 혼용하지 않는다.
