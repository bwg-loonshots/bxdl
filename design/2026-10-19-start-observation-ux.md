# 시작 상태와 관측 기한 UX

2026-10-19 · Rust/macOS arm64 · [NIGO J1~J3 수신](../contracts/nigo/jre-feedback-2026-10-19/README.md).

## 문제와 사용자 흐름

엔진은 관측 시간이 끝난 뒤에도 시작 준비를 계속할 수 있다. 기존 start는 마지막 짧은 상태 조회의 예산이 소진되면
`SERVICE_OPERATION_UNKNOWN` 오류만 내보내 시도 ID와 STARTING 근거를 잃을 수 있었다.
운영자는 기한 만료, 실제 엔진 실패, 준비 완료를 구분하고 같은 실행을 이어서 확인해야 한다.

1. `start`는 기존처럼 한 번 실행을 요청하고 기본 120초 동안 준비 완료를 관측한다. 지정 시간은 시작 전 cold 명령(최대120초)에도 적용되며, bootstrap 뒤 관측은 별도 예산이다.
2. 대기가 끝나면 준비 완료로 판정하지 않고 마지막 서비스/엔진/준비 상태와 시도 ID를 보여준다. 작업을 취소하거나 재시작하지 않는다.
3. `status`는 한 번 조회한다. 새 `status --wait`는 조회를 시작할 때 고정한 현재 시도만 최대 지정 시간 동안 관측한다.
4. 설치 도우미의 `check`는 현재 상태를 재검증하고, 관측된 시작 시도가 있으면 `wait`로 준비 상태를 기다린다.
5. READY/실제 FAILED/종료에 도달하면 대기를 끝낸다. 도중에 다른 실행 시도로 바뀌면 자동으로 따라가지 않는다.

## 계약

- `status --wait`: 기본 120초, 1~600초. 단발 status는 기본10초/1~120초, start·stop의 기존 기본값도 유지한다.
- `--wait`는 status 전용이다. 기한은 공통 예산이며 다음 조회 때 새 전체 예산을 부여하지 않는다.
- `SERVICE_START_WAIT_EXPIRED`와 `SERVICE_STATUS_WAIT_EXPIRED`는 준비 상태의 관측 기한 종료다.
  `lastObservationReason`은 마지막 실제 관측 사유를 보존한다. UNKNOWN/INCOMPLETE를 성공이나 엔진 실패로 변환하지 않는다.
- 단발 조회 자체의 예산 소진은 `SERVICE_STATUS_QUERY_EXPIRED`, 관측 중 시도 변경은 `SERVICE_ATTEMPT_CHANGED`로 구분한다.
- 엔진 FAILED·gate 거부·종료 근거는 기한 만료로 덮지 않는다. report/HTTP identity 불일치를 단순 시간 초과로 위장하지 않는다.
- JSON은 기존 envelope/exit 의미를 유지한다. 사람용 화면은 고정한 안전한 필드만 한국어로 표시한다.
- `wait`는 새 bootstrap/init/start/stop·상태 수리 작업을 수행하지 않는다. 설치 도우미의 진행 기록과 실제 관측을 구분한다.
- 기존 journal/plan/hash·엔진 OperationLock·JAR/JRE·LaunchAgent profile은 유지한다. 조회 실패 시 이전 READY를 현재 상태로 재사용하지 않는다.

## 검증

기한 경계에서 시도/STARTING 보존, 지연 READY와 실제 실패의 구분, 시도 변경 중단, 대기 중 새 예산/실행 없음,
JSON과 사람용 출력, 설치 세션 재개와 중복 start 차단을 자동 검사한다.
실제 새 signed package에서는 정상 시작 요청 이후 `status --wait`의 짧은 관측 기한 만료를 의도적으로 만든 뒤 같은 시도의 읽기 대기·READY·정상 종료·재개를 검증한다.
이는 이전 15초 시험의 판정을 바꾸거나 엔진 기동 SLA를 정하는 시험이 아니다. 실제 결과는 [별도 검증 기록](../results/2026-10-19-start-observation-ux.md)에 남겼다.

## 후속

source-free native/PKI 준비, 실제 package의 4-validator 합의, 로그인/로그아웃·sleep/wake·중단/실패 전체 G1-M과
J5 공급망·최소 OS·Linux/systemd·Docker는 별도 후속이다.
