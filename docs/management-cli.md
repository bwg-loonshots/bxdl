# 관리 CLI 독립 업데이트

새 관리 CLI를 설치 package 밖에서 실행해도 검증된 기존 RC.3 package를 운용할 수 있다. package 내부의 `bin/bxdl`·NIGO JAR·JRE를 덮어쓰거나 archive를 다시 만들지 않는다. 관리 CLI는 대상과 입력을 검사한 뒤 해당 package의 실행기에 고정 명령·JSON으로 위임한다.

```sh
/path/to/new/bxdl nodes add local --instance /path/to/control
/path/to/new/bxdl status local
/path/to/new/bxdl start local
/path/to/new/bxdl status local --wait
/path/to/new/bxdl logs local
/path/to/new/bxdl diagnose local --output /path/to/new-report.json
/path/to/new/bxdl stop local
```

별도 목록을 쓰면 이름 명령에도 동일한 `--home`을 지정한다. 이름과 `--instance`는 둘 중 하나만 받는다. 기존 경로 명령은 catalog가 없어도 동작하며 `--home`을 섞지 않는다. 관리 CLI를 바꿔도 사용자 목록·control·data를 다시 만들지 않는다.

개발 checkout에서는 `make build`로 만든 `bin/bxdl`을 외부 관리 CLI로 사용한다. 배포 시에도 package 밖의 관리 CLI 실행 파일만 교체한다. `bxdl version --json`의 version/revision은 현재 관리 CLI의 식별 정보이며 NIGO 엔진 버전과 별도로 관리한다. 기존 이름 목록은 같은 사용자/관리 home에서 이어 읽는다.

## 지원 범위

첫 호환 profile은 [RC.3 인수 기록](../results/2026-10-22-macos-rc3-package.md)의 archive `8a45cb8b…`, 실행기 `b7d677b4…` 조합이다. 다른 RC.3 재빌드나 같은 `0.1.0-dev` 문자열은 이 profile에 자동 포함되지 않는다. [분리 설계](../design/2026-10-23-independent-management-cli.md)에 전체 hash·계약을 기록했다.

등록된 RC.3의 `instance show`, `preflight`, `init`/`resume-init`, `start`, `status`/`--wait`, `stop`, `logs`, `diagnose`를 위임한다. init/resume-init은 기존 명시 확인 플래그가 필요하다. 최초 설치와 등록은 기존 API를 사용하며, `setup --install`도 이 RC.3를 선택해 노드 작업을 내부 실행기에 위임할 수 있다. `setup new/resume`은 순차 문답으로 자료를 준비하며, `setup apply <이름>`은 TTY 승인 뒤 기존 installer/등록 API로 설치·cold 검사·등록·이름 연결까지 진행한다. `init <이름> --confirm-initialize`와 조건부 `resume-init <이름> --confirm-resume`도 이름 또는 기존 `--instance`를 선택해 같은 고정 RC.3 실행기에 위임한다. 초기화와 start를 설치 승인에 묶지 않는다.

미지원 package는 새 CLI가 임의로 내부 실행기를 실행하지 않는다. 기존 native 경로의 조건을 적용하며, 특히 start는 기존과 같이 package CLI 일치를 요구한다. 지원 profile에서 서명·파일 검증이 실패하면 실행 방식 변경이나 자동 복구 없이 거절한다.

[실제 Mac 검증](../results/2026-10-23-independent-management-cli.md)에서 두 관리 CLI로 같은 기존 RC.3의 직접 운용16사례와 설치 도우미 재개8사례를 확인했다. 이번 READY는 단일 노드의 MANUAL_START_INITIALIZED이며 합의·거래 확정을 보장하지 않는다.

## 동작과 결과

명령·JSON envelope·exit 의미는 기존 계약을 유지한다. service의 READY는 로컬 상태이며 합의 전체의 확인이 아니다. logs/diagnose는 저장 기록이고 현재 runtime을 관측하지 않는다. 위임 전 archive와 설치 inventory 인증 비용도 있으므로 `--timeout-seconds`를 전체 명령의 벽시계 상한으로 해석하지 않는다.

작업이 실행기에 전달된 뒤 결과를 확정할 수 없으면 변경 가능 작업은 `EXECUTOR_OPERATION_UNKNOWN`/exit 6으로 종료한다. 원문 출력은 노출하지 않으며 자동으로 재시도·초기화·start하지 않는다. 관리 CLI 종료/timeout은 노드 중지나 작업 취소를 보장하지 않는다. `status`와 기존 기록을 확인한다.

노드의 실제 초기화·시작 journal과 worker snapshot은 기존 실행기가 작성한다. 구형 실행기의 stop은 잠금을 얻었을 때 현재 실행 시도를 대상으로 한다. 관리 CLI는 이름의 root/binding을 직전 재검사하지만, 구형 실행기가 지원하지 않는 원자적 expected-attempt 승인 전달을 제공한다고 주장하지 않는다.

## 업데이트 기준

- 이름·도움말·표시·문답 개선: 관리 CLI만 교체하고 지원 runtime과의 호환 조합을 검사한다.
- 실행기 동작/보안 수정 또는 NIGO/JRE 변경: 새 runtime package를 만들고 인수한다.
- 기존 노드 DB·control 이동·upgrade/migration은 별도 작업이다. 관리 CLI 교체가 이를 수행하지 않는다.

현재의 사용자 LaunchAgent 운영 범위는 그대로다. 별도 상주 관리 서버나 자동 재시작 기능을 추가하지 않는다.
